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

#[derive(Message, Debug, Clone, Copy)]
pub struct ChunkColliderRebuildRequest {
    pub chunk_pos: ChunkPos,
    pub lod: usize,
}

impl ChunkColliderRebuildRequest {
    pub fn new(chunk_pos: ChunkPos) -> Self {
        Self { chunk_pos, lod: 1 }
    }

    pub fn with_lod(chunk_pos: ChunkPos, lod: usize) -> Self {
        Self { chunk_pos, lod }
    }
}

#[derive(Resource, Default)]
pub struct ChunkColliderEntityRegistry {
    pub entities: HashMap<ChunkPos, Entity>,
}

#[derive(Resource, Default)]
pub struct ColliderCookingInProgress {
    pub chunks: HashSet<ChunkPos>,
}

#[derive(Resource, Default)]
pub struct ColliderCookQueue {
    pub pending: HashMap<ChunkPos, usize>,
}

#[derive(Resource, Default)]
pub struct ColliderVersionTracker {
    pub versions: HashMap<ChunkPos, u64>,
}

#[derive(Resource)]
pub struct ColliderCookJobSender(pub Sender<ColliderCookJob>);

#[derive(Resource)]
pub struct ColliderCookResultReceiver(pub Receiver<ColliderCookResult>);

pub struct ColliderCookJob {
    chunk_pos: ChunkPos,
    chunk: Arc<Chunk>,
    lod: usize,
    version: u64,
}

pub struct ColliderCookResult {
    chunk_pos: ChunkPos,
    bundle: Option<StaticChunkColliderBundle>,
    version: u64,
}

pub trait ChunkProvider: Send + Sync + 'static {
    fn get_chunk(&self, pos: ChunkPos) -> Option<Arc<Chunk>>;
}

const MAX_COOK_JOBS_PER_TICK: usize = 8;
const COOK_JOB_QUEUE_CAPACITY: usize = 128;
const COLLIDER_COOK_THREAD_COUNT: usize = 2;
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

    for event in events.read() {
        cook_queue
            .pending
            .entry(event.chunk_pos)
            .and_modify(|existing_lod| *existing_lod = (*existing_lod).min(event.lod))
            .or_insert(event.lod);
    }

    let mut dispatched = 0usize;

    let mut to_start: Vec<(ChunkPos, usize)> = cook_queue
        .pending
        .iter()
        .filter(|(pos, _)| !cooking_in_progress.chunks.contains(*pos))
        .map(|(pos, lod)| (*pos, *lod))
        .take(MAX_COOK_JOBS_PER_TICK)
        .collect();

    for (chunk_pos, lod) in to_start.drain(..) {
        let Some(chunk) = chunk_provider.get_chunk(chunk_pos) else {
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
        let current_version = version_tracker
            .versions
            .get(&chunk_pos)
            .copied()
            .unwrap_or(0);
        if result_version != current_version {
            cooking_in_progress.chunks.remove(&chunk_pos);
            continue;
        }

        if let Some(old_entity) = collider_entities.entities.remove(&chunk_pos) {
            commands.entity(old_entity).despawn();
        }

        if let Some(bundle) = bundle {
            let entity = commands.spawn(bundle).id();
            collider_entities.entities.insert(chunk_pos, entity);
        }

        cooking_in_progress.chunks.remove(&chunk_pos);

        spawned += 1;
        if spawned >= MAX_COLLIDER_SPAWNS_PER_TICK {
            break;
        }
    }
}

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
            cook_queue.pending.remove(&chunk_pos);
            cooking_in_progress.chunks.remove(&chunk_pos);
            version_tracker.versions.remove(&chunk_pos);

            if let Some(entity) = collider_entities.entities.remove(&chunk_pos) {
                commands.entity(entity).despawn();
            }
        }
    }
}

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
