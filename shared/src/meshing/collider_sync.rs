//! Systems for synchronizing chunk colliders with the world.
//!
//! This module provides Bevy systems that automatically spawn and despawn
//! chunk collider entities as chunks are loaded and unloaded. It's designed
//! to work with both client and server, receiving chunk change notifications
//! via events.

use bevy::prelude::*;
use crossbeam::channel::{bounded, unbounded, Receiver, Sender, TrySendError};
use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::thread::Builder;

use crate::world::chunk::Chunk;
use crate::world::pos::pos2d::chunks_in_col;
use crate::world::pos::pos3d::ChunkPos;
use crate::world::ColUnloadEvent;

use super::chunk_collider::generate_chunk_trimesh_collider;
use super::chunk_collider::StaticChunkColliderBundle;

/// Event requesting that a chunk's static physics collider be (re)generated.
///
/// Sent when:
/// - A new chunk is loaded and needs an initial collider
/// - An existing chunk's blocks changed and the collider needs rebuilding
#[derive(Message, Debug, Clone, Copy)]
pub struct ChunkColliderRebuildRequest {
    pub chunk_pos: ChunkPos,
    /// Level of detail for the collider (1 = full detail, higher = more simplified).
    /// Defaults to 1 if not specified.
    pub lod: usize,
}

impl ChunkColliderRebuildRequest {
    /// Create a new rebuild request with full detail (LOD 1).
    pub fn new(chunk_pos: ChunkPos) -> Self {
        Self { chunk_pos, lod: 1 }
    }

    /// Create a new rebuild request with specified LOD.
    pub fn with_lod(chunk_pos: ChunkPos, lod: usize) -> Self {
        Self { chunk_pos, lod }
    }
}

/// Registry mapping chunk positions to their static physics collider entities.
///
/// This resource tracks which Entity is the physics collider for each chunk,
/// allowing efficient lookup when colliders need to be updated or removed.
#[derive(Resource, Default)]
pub struct ChunkColliderEntityRegistry {
    pub entities: HashMap<ChunkPos, Entity>,
}

/// Tracks which chunks currently have collider cooking jobs running on worker threads.
#[derive(Resource, Default)]
pub struct ColliderCookingInProgress {
    pub chunks: HashSet<ChunkPos>,
}

/// Queue of chunks waiting to have their colliders cooked.
/// Maps chunk position to the requested LOD level.
#[derive(Resource, Default)]
pub struct ColliderCookQueue {
    pub pending: HashMap<ChunkPos, usize>,
}

/// Tracks collider version per chunk to discard stale cook results.
/// When a chunk is modified while cooking, the version increments and
/// the old cook result (with lower version) is discarded.
#[derive(Resource, Default)]
pub struct ColliderVersionTracker {
    pub versions: HashMap<ChunkPos, u64>,
}

#[derive(Resource)]
pub struct ColliderCookJobSender(pub Sender<ColliderCookJob>);

#[derive(Resource)]
pub struct ColliderCookResultReceiver(pub Receiver<ColliderCookResult>);

struct ColliderCookJob {
    chunk_pos: ChunkPos,
    chunk: Arc<Chunk>,
    lod: usize,
    version: u64,
}

struct ColliderCookResult {
    chunk_pos: ChunkPos,
    bundle: Option<StaticChunkColliderBundle>,
    version: u64,
}

/// Trait for accessing chunk data. Implemented by both ClientWorldMap and VoxelWorld.
pub trait ChunkProvider: Send + Sync + 'static {
    /// Get a chunk if it exists, returning a clone for thread safety.
    fn get_chunk(&self, pos: ChunkPos) -> Option<Arc<Chunk>>;
}

/// Maximum number of new collider cook jobs to dispatch per frame.
const MAX_COOK_JOBS_PER_TICK: usize = 8;
/// Capacity for the collider cook job queue.
const COOK_JOB_QUEUE_CAPACITY: usize = 128;
/// Number of threads dedicated to collider cooking.
const COLLIDER_COOK_THREAD_COUNT: usize = 2;
/// Maximum number of cooked colliders to spawn per frame to avoid long stalls.
const MAX_COLLIDER_SPAWNS_PER_TICK: usize = 16;

fn spawn_collider_cook_worker_threads(mut commands: Commands) {
    let (job_sender, job_receiver) = bounded::<ColliderCookJob>(COOK_JOB_QUEUE_CAPACITY);
    let (result_sender, result_receiver) = unbounded::<ColliderCookResult>();

    for i in 0..COLLIDER_COOK_THREAD_COUNT {
        let job_receiver = job_receiver.clone();
        let result_sender = result_sender.clone();
        Builder::new()
            .name(format!("collider-cook-{i}"))
            .spawn(move || {
                while let Ok(job) = job_receiver.recv() {
                    // Catch panics during collider cooking to avoid killing the worker thread.
                    let cooked = catch_unwind(AssertUnwindSafe(|| {
                        generate_chunk_trimesh_collider(&job.chunk, job.lod).map(|collider| {
                            StaticChunkColliderBundle::from_collider(collider, job.chunk_pos)
                        })
                    }));

                    let bundle = match cooked {
                        Ok(bundle) => bundle,
                        Err(_) => {
                            warn!(
                                "Collider cook panicked for chunk {:?}; discarding result",
                                job.chunk_pos
                            );
                            None
                        }
                    };

                    if result_sender
                        .send(ColliderCookResult {
                            chunk_pos: job.chunk_pos,
                            bundle,
                            version: job.version,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("collider cook thread spawn");
    }

    commands.insert_resource(ColliderCookJobSender(job_sender));
    commands.insert_resource(ColliderCookResultReceiver(result_receiver));
}

/// Dispatches collider cook jobs to worker threads from the pending queue.
pub fn dispatch_collider_cook_jobs<P: ChunkProvider + Resource>(
    mut events: MessageReader<ChunkColliderRebuildRequest>,
    chunk_provider: Option<Res<P>>,
    mut cook_queue: ResMut<ColliderCookQueue>,
    mut cooking_in_progress: ResMut<ColliderCookingInProgress>,
    mut version_tracker: ResMut<ColliderVersionTracker>,
    job_sender: Res<ColliderCookJobSender>,
) {
    let Some(chunk_provider) = chunk_provider else {
        return;
    };

    // Coalesce incoming events; we only need one pending entry per chunk.
    // If multiple requests come for the same chunk, use the lowest (most detailed) LOD.
    for event in events.read() {
        cook_queue
            .pending
            .entry(event.chunk_pos)
            .and_modify(|existing_lod| *existing_lod = (*existing_lod).min(event.lod))
            .or_insert(event.lod);
    }

    let mut dispatched = 0usize;

    // Start up to the frame budget from the pending set, skipping chunks already cooking
    let mut to_start: Vec<(ChunkPos, usize)> = cook_queue
        .pending
        .iter()
        .filter(|(pos, _)| !cooking_in_progress.chunks.contains(*pos))
        .map(|(pos, lod)| (*pos, *lod))
        .take(MAX_COOK_JOBS_PER_TICK)
        .collect();

    for (chunk_pos, lod) in to_start.drain(..) {
        let Some(chunk) = chunk_provider.get_chunk(chunk_pos) else {
            // Keep pending if chunk not available; will retry later
            continue;
        };

        let version_entry = version_tracker.versions.entry(chunk_pos).or_insert(0);
        *version_entry += 1;
        let version = *version_entry;

        match job_sender.0.try_send(ColliderCookJob {
            chunk_pos,
            chunk,
            lod,
            version,
        }) {
            Ok(()) => {
                cook_queue.pending.remove(&chunk_pos);
                cooking_in_progress.chunks.insert(chunk_pos);
                dispatched += 1;
            }
            Err(TrySendError::Full(_)) => {
                // Worker queue is full; keep pending and try again next tick
                warn!("Collider cook queue is full; deferring new jobs");
                break;
            }
            Err(TrySendError::Disconnected(_)) => {
                warn!("Collider cook worker is disconnected; dropping job for {chunk_pos:?}");
                break;
            }
        }

        if dispatched >= MAX_COOK_JOBS_PER_TICK {
            break;
        }
    }
}

/// Spawns collider entities from finished cook results.
pub fn spawn_cooked_collider_entities(
    mut commands: Commands,
    mut collider_entities: ResMut<ChunkColliderEntityRegistry>,
    mut cooking_in_progress: ResMut<ColliderCookingInProgress>,
    version_tracker: Res<ColliderVersionTracker>,
    results: Option<Res<ColliderCookResultReceiver>>,
) {
    let Some(results) = results else {
        return;
    };

    let mut spawned = 0usize;

    for ColliderCookResult {
        chunk_pos,
        bundle,
        version: result_version,
    } in results.0.try_iter()
    {
        // Drop stale results if a newer version was queued for the same chunk
        let current_version = version_tracker
            .versions
            .get(&chunk_pos)
            .copied()
            .unwrap_or(0);
        if result_version != current_version {
            cooking_in_progress.chunks.remove(&chunk_pos);
            continue;
        }

        // Remove old collider entity if present
        if let Some(old_entity) = collider_entities.entities.remove(&chunk_pos) {
            commands.entity(old_entity).despawn();
        }

        // Spawn new collider if one was generated
        if let Some(bundle) = bundle {
            let entity = commands.spawn(bundle).id();
            collider_entities.entities.insert(chunk_pos, entity);
        }

        // Mark cooking complete
        cooking_in_progress.chunks.remove(&chunk_pos);

        spawned += 1;
        if spawned >= MAX_COLLIDER_SPAWNS_PER_TICK {
            break;
        }
    }
}

/// Despawns chunk collider entities when their containing column is unloaded.
pub fn despawn_colliders_for_unloaded_columns(
    mut commands: Commands,
    mut events: MessageReader<ColUnloadEvent>,
    mut collider_entities: ResMut<ChunkColliderEntityRegistry>,
    mut cook_queue: ResMut<ColliderCookQueue>,
    mut cooking_in_progress: ResMut<ColliderCookingInProgress>,
    mut version_tracker: ResMut<ColliderVersionTracker>,
) {
    for event in events.read() {
        for chunk_pos in chunks_in_col(&event.0) {
            // Cancel any pending or in-progress cook task for this chunk
            cook_queue.pending.remove(&chunk_pos);
            cooking_in_progress.chunks.remove(&chunk_pos);
            version_tracker.versions.remove(&chunk_pos);

            if let Some(entity) = collider_entities.entities.remove(&chunk_pos) {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Plugin that manages chunk colliders.
///
/// This plugin sets up the systems needed to automatically maintain chunk
/// colliders as the world changes. It requires the physics plugin to be
/// added separately.
///
/// Note: This plugin does NOT register `ColUnloadEvent` - the caller must
/// ensure it's registered (typically via their world plugin) since it's
/// a shared event used by multiple systems.
pub struct ChunkColliderPlugin<P: ChunkProvider + Resource>(std::marker::PhantomData<P>);

impl<P: ChunkProvider + Resource> Default for ChunkColliderPlugin<P> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<P: ChunkProvider + Resource> Plugin for ChunkColliderPlugin<P> {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkColliderEntityRegistry>()
            .init_resource::<ColliderCookQueue>()
            .init_resource::<ColliderCookingInProgress>()
            .init_resource::<ColliderVersionTracker>()
            .add_systems(Startup, spawn_collider_cook_worker_threads)
            .add_message::<ChunkColliderRebuildRequest>()
            .add_systems(
                Update,
                (
                    dispatch_collider_cook_jobs::<P>,
                    despawn_colliders_for_unloaded_columns,
                    spawn_cooked_collider_entities,
                )
                    .chain(),
            );
    }
}
