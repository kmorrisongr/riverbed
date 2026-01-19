//! Systems for synchronizing chunk colliders with the world.
//!
//! This module provides Bevy systems that automatically spawn and despawn
//! chunk collider entities as chunks are loaded and unloaded. It's designed
//! to work with both client and server, receiving chunk change notifications
//! via events.

use bevy::prelude::*;
use crossbeam::channel::{bounded, unbounded, Receiver, Sender, TrySendError};
use std::collections::{HashMap, HashSet};
use std::thread::Builder;
use std::sync::Arc;

use crate::world::chunk::Chunk;
use crate::world::pos::pos2d::chunks_in_col;
use crate::world::pos::pos3d::ChunkPos;
use crate::world::ColUnloadEvent;

use super::chunk_collider::StaticChunkColliderBundle;
use super::chunk_collider::generate_chunk_trimesh_collider;

/// Event requesting that a chunk's static physics collider be (re)generated.
///
/// Sent when:
/// - A new chunk is loaded and needs an initial collider
/// - An existing chunk's blocks changed and the collider needs rebuilding
#[derive(Message, Debug, Clone, Copy)]
pub struct ChunkColliderRebuildRequest {
    pub chunk_pos: ChunkPos,
}

/// Registry mapping chunk positions to their static physics collider entities.
///
/// This resource tracks which Entity is the physics collider for each chunk,
/// allowing efficient lookup when colliders need to be updated or removed.
#[derive(Resource, Default)]
pub struct ChunkColliderEntityRegistry {
    pub entities: HashMap<ChunkPos, Entity>,
}

/// Chunks that are currently being cooked by the worker.
#[derive(Resource, Default)]
pub struct ChunkColliderInFlight {
    pub in_flight: HashSet<ChunkPos>,
}

/// Pending chunk positions waiting to be cooked (coalesced per chunk).
#[derive(Resource, Default)]
pub struct ChunkColliderPending {
    pub pending: HashSet<ChunkPos>,
}

#[derive(Resource)]
pub struct ChunkColliderJobSender(pub Sender<ChunkColliderJob>);

#[derive(Resource)]
pub struct ChunkColliderResultReceiver(pub Receiver<ChunkColliderResult>);

struct ChunkColliderJob {
    chunk_pos: ChunkPos,
    chunk: Arc<Chunk>,
}

struct ChunkColliderResult {
    chunk_pos: ChunkPos,
    bundle: Option<StaticChunkColliderBundle>,
}

/// Trait for accessing chunk data. Implemented by both ClientWorldMap and VoxelWorld.
pub trait ChunkProvider: Send + Sync + 'static {
    /// Get a chunk if it exists, returning a clone for thread safety.
    fn get_chunk(&self, pos: ChunkPos) -> Option<Arc<Chunk>>;
}

/// Maximum number of new collider cook tasks to start per frame.
const MAX_NEW_COOK_TASKS_PER_TICK: usize = 8;
/// Capacity for the collider worker's input queue.
const COLLIDER_QUEUE_CAPACITY: usize = 128;
/// Number of threads dedicated to collider cooking.
const COLLIDER_WORKER_THREADS: usize = 2;
/// Maximum number of cooked colliders to apply per frame to avoid long stalls.
const MAX_COLLIDER_APPLIES_PER_TICK: usize = 16;

fn setup_collider_worker(mut commands: Commands) {
    let (job_sender, job_receiver) = bounded::<ChunkColliderJob>(COLLIDER_QUEUE_CAPACITY);
    let (result_sender, result_receiver) = unbounded::<ChunkColliderResult>();

    for i in 0..COLLIDER_WORKER_THREADS {
        let job_receiver = job_receiver.clone();
        let result_sender = result_sender.clone();
        Builder::new()
            .name(format!("collider-worker-{i}"))
            .spawn(move || {
                while let Ok(job) = job_receiver.recv() {
                    let bundle = generate_chunk_trimesh_collider(&job.chunk).map(|collider| {
                        StaticChunkColliderBundle::from_collider(collider, job.chunk_pos)
                    });

                    if result_sender
                        .send(ChunkColliderResult {
                            chunk_pos: job.chunk_pos,
                            bundle,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("collider worker thread spawn");
    }

    commands.insert_resource(ChunkColliderJobSender(job_sender));
    commands.insert_resource(ChunkColliderResultReceiver(result_receiver));
}

/// System that enqueues asynchronous collider cook tasks for chunk rebuild requests.
pub fn queue_collider_cook_tasks<P: ChunkProvider + Resource>(
    mut events: MessageReader<ChunkColliderRebuildRequest>,
    chunk_provider: Option<Res<P>>,
    mut pending: ResMut<ChunkColliderPending>,
    mut in_flight: ResMut<ChunkColliderInFlight>,
    job_sender: Res<ChunkColliderJobSender>,
) {
    let Some(chunk_provider) = chunk_provider else {
        return;
    };

    // Coalesce incoming events; we only need one pending entry per chunk.
    for event in events.read() {
        pending.pending.insert(event.chunk_pos);
    }

    let mut spawned = 0usize;

    // Start up to the frame budget from the pending set, skipping chunks already cooking
    let mut to_start: Vec<ChunkPos> = pending
        .pending
        .iter()
        .filter(|pos| !in_flight.in_flight.contains(*pos))
        .cloned()
        .take(MAX_NEW_COOK_TASKS_PER_TICK)
        .collect();

    for chunk_pos in to_start.drain(..) {
        let Some(chunk) = chunk_provider.get_chunk(chunk_pos) else {
            // Keep pending if chunk not available; will retry later
            continue;
        };

        match job_sender.0.try_send(ChunkColliderJob { chunk_pos, chunk }) {
            Ok(()) => {
                pending.pending.remove(&chunk_pos);
                in_flight.in_flight.insert(chunk_pos);
                spawned += 1;
            }
            Err(TrySendError::Full(_)) => {
                // Worker queue is full; keep pending and try again next tick
                warn!("Collider worker queue is full; deferring new jobs");
                break;
            }
            Err(TrySendError::Disconnected(_)) => {
                warn!("Collider worker thread is disconnected; dropping job for {chunk_pos:?}");
                break;
            }
        }

        if spawned >= MAX_NEW_COOK_TASKS_PER_TICK {
            break;
        }
    }
}

/// System that applies finished collider cook tasks, spawning/despawning entities on the main thread.
pub fn apply_finished_collider_cooks(
    mut commands: Commands,
    mut collider_entities: ResMut<ChunkColliderEntityRegistry>,
    mut in_flight: ResMut<ChunkColliderInFlight>,
    results: Option<Res<ChunkColliderResultReceiver>>,
) {
    let Some(results) = results else {
        return;
    };

    let mut applied = 0usize;

    for ChunkColliderResult { chunk_pos, bundle } in results.0.try_iter() {
        // Remove old collider entity if present
        if let Some(old_entity) = collider_entities.entities.remove(&chunk_pos) {
            commands.entity(old_entity).despawn();
        }

        // Spawn new collider if one was generated
        if let Some(bundle) = bundle {
            let entity = commands.spawn(bundle).id();
            collider_entities.entities.insert(chunk_pos, entity);
        }

        // Mark job complete
        in_flight.in_flight.remove(&chunk_pos);

        applied += 1;
        if applied >= MAX_COLLIDER_APPLIES_PER_TICK {
            break;
        }
    }
}

/// System that despawns chunk collider entities when their containing column is unloaded.
///
/// Listens for `ColUnloadEvent` and removes all collider entities for chunks in that column.
pub fn despawn_colliders_for_unloaded_columns(
    mut commands: Commands,
    mut events: MessageReader<ColUnloadEvent>,
    mut collider_entities: ResMut<ChunkColliderEntityRegistry>,
    mut pending: ResMut<ChunkColliderPending>,
    mut in_flight: ResMut<ChunkColliderInFlight>,
) {
    for event in events.read() {
        for chunk_pos in chunks_in_col(&event.0) {
            // Cancel any pending or in-flight cook task for this chunk
            pending.pending.remove(&chunk_pos);
            in_flight.in_flight.remove(&chunk_pos);

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
            .init_resource::<ChunkColliderPending>()
            .init_resource::<ChunkColliderInFlight>()
            .add_systems(Startup, setup_collider_worker)
            .add_message::<ChunkColliderRebuildRequest>()
            .add_systems(
                Update,
                (
                    queue_collider_cook_tasks::<P>,
                    despawn_colliders_for_unloaded_columns,
                    apply_finished_collider_cooks,
                )
                    .chain(),
            );
    }
}
