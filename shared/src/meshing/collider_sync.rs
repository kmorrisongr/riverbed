//! Systems for synchronizing chunk colliders with the world.
//!
//! This module provides Bevy systems that automatically spawn and despawn
//! chunk collider entities as chunks are loaded and unloaded. It's designed
//! to work with both client and server, receiving chunk change notifications
//! via events.

use bevy::prelude::*;
use bevy::tasks::{futures_lite, AsyncComputeTaskPool, Task};
use futures_lite::future::poll_once;
use std::collections::{HashMap, HashSet};

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

/// In-flight collider cook tasks keyed by chunk position.
#[derive(Resource, Default)]
pub struct ChunkColliderCookTasks {
    pub tasks: HashMap<ChunkPos, Task<Option<StaticChunkColliderBundle>>>,
}

/// Pending chunk positions waiting to be cooked (coalesced per chunk).
#[derive(Resource, Default)]
pub struct ChunkColliderPending {
    pub pending: HashSet<ChunkPos>,
}

/// Trait for accessing chunk data. Implemented by both ClientWorldMap and VoxelWorld.
pub trait ChunkProvider: Send + Sync + 'static {
    /// Get a chunk if it exists, returning a clone for thread safety.
    fn get_chunk(&self, pos: ChunkPos) -> Option<Chunk>;
}

/// Maximum number of new collider cook tasks to start per frame.
const MAX_NEW_COOK_TASKS_PER_TICK: usize = 8;

/// System that enqueues asynchronous collider cook tasks for chunk rebuild requests.
pub fn queue_collider_cook_tasks<P: ChunkProvider + Resource>(
    mut events: MessageReader<ChunkColliderRebuildRequest>,
    chunk_provider: Option<Res<P>>,
    mut pending: ResMut<ChunkColliderPending>,
    mut cook_tasks: ResMut<ChunkColliderCookTasks>,
) {
    let Some(chunk_provider) = chunk_provider else {
        return;
    };

    // Coalesce incoming events; we only need one pending entry per chunk.
    for event in events.read() {
        pending.pending.insert(event.chunk_pos);
    }

    let pool = AsyncComputeTaskPool::get();
    let mut spawned = 0usize;

    // Start up to the frame budget from the pending set, skipping chunks already cooking
    let mut to_start: Vec<ChunkPos> = pending
        .pending
        .iter()
        .filter(|pos| !cook_tasks.tasks.contains_key(*pos))
        .cloned()
        .take(MAX_NEW_COOK_TASKS_PER_TICK)
        .collect();

    for chunk_pos in to_start.drain(..) {
        // Replace any in-flight task for the same chunk with the latest request
        cook_tasks.tasks.remove(&chunk_pos);

        let Some(chunk) = chunk_provider.get_chunk(chunk_pos) else {
            // Keep pending if chunk not available; will retry later
            continue;
        };

        let task = pool.spawn(async move {
            generate_chunk_trimesh_collider(&chunk)
                .map(|collider| StaticChunkColliderBundle::from_collider(collider, chunk_pos))
        });

        cook_tasks.tasks.insert(chunk_pos, task);
        pending.pending.remove(&chunk_pos);
        spawned += 1;

        if spawned >= MAX_NEW_COOK_TASKS_PER_TICK {
            break;
        }
    }
}

/// System that applies finished collider cook tasks, spawning/despawning entities on the main thread.
pub fn apply_finished_collider_cooks(
    mut commands: Commands,
    mut cook_tasks: ResMut<ChunkColliderCookTasks>,
    mut collider_entities: ResMut<ChunkColliderEntityRegistry>,
) {
    let mut finished = Vec::new();

    for (chunk_pos, task) in cook_tasks.tasks.iter_mut() {
        if let Some(result) = futures_lite::future::block_on(poll_once(task)) {
            finished.push((*chunk_pos, result));
        }
    }

    for (chunk_pos, bundle) in finished {
        // Remove old collider entity if present
        if let Some(old_entity) = collider_entities.entities.remove(&chunk_pos) {
            commands.entity(old_entity).despawn();
        }

        // Spawn new collider if one was generated
        if let Some(bundle) = bundle {
            let entity = commands.spawn(bundle).id();
            collider_entities.entities.insert(chunk_pos, entity);
        }

        // Drop completed task
        cook_tasks.tasks.remove(&chunk_pos);
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
    mut cook_tasks: ResMut<ChunkColliderCookTasks>,
) {
    for event in events.read() {
        for chunk_pos in chunks_in_col(&event.0) {
            // Cancel any pending or in-flight cook task for this chunk
            pending.pending.remove(&chunk_pos);
            cook_tasks.tasks.remove(&chunk_pos);

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
            .init_resource::<ChunkColliderCookTasks>()
            .add_message::<ChunkColliderRebuildRequest>()
            .add_systems(Update, queue_collider_cook_tasks::<P>)
            .add_systems(Update, apply_finished_collider_cooks)
            .add_systems(Update, despawn_colliders_for_unloaded_columns);
    }
}
