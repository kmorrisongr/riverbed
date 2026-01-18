//! Systems for synchronizing chunk colliders with the world.
//!
//! This module provides Bevy systems that automatically spawn and despawn
//! chunk collider entities as chunks are loaded and unloaded. It's designed
//! to work with both client and server, receiving chunk change notifications
//! via events.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::world::chunk::Chunk;
use crate::world::pos::pos2d::chunks_in_col;
use crate::world::pos::pos3d::ChunkPos;
use crate::world::ColUnloadEvent;

use super::chunk_collider::ChunkColliderBundle;

/// Event requesting that a chunk's physics collider be generated or regenerated.
///
/// Sent when:
/// - A new chunk is loaded and needs an initial collider
/// - An existing chunk's blocks changed and the collider needs rebuilding
#[derive(Message, Debug, Clone, Copy)]
pub struct RebuildChunkColliderRequest {
    pub chunk_pos: ChunkPos,
}

/// Maps chunk positions to their physics collider entities.
///
/// This resource tracks which Entity is the physics collider for each chunk,
/// allowing efficient lookup when colliders need to be updated or removed.
#[derive(Resource, Default)]
pub struct ChunkColliderEntityMap {
    pub entities: HashMap<ChunkPos, Entity>,
}

/// Trait for accessing chunk data. Implemented by both ClientWorldMap and VoxelWorld.
pub trait ChunkProvider: Send + Sync + 'static {
    /// Get a chunk if it exists, returning a clone for thread safety.
    fn get_chunk(&self, pos: ChunkPos) -> Option<Chunk>;
}

/// System that spawns or updates chunk collider entities when chunks change.
pub fn handle_chunk_collider_rebuild_requests<P: ChunkProvider + Resource>(
    mut commands: Commands,
    mut events: MessageReader<RebuildChunkColliderRequest>,
    chunk_provider: Option<Res<P>>,
    mut collider_entities: ResMut<ChunkColliderEntityMap>,
) {
    let Some(chunk_provider) = chunk_provider else {
        return;
    };

    for event in events.read() {
        let chunk_pos = event.chunk_pos;

        // Remove existing collider entity if present
        if let Some(old_entity) = collider_entities.entities.remove(&chunk_pos) {
            commands.entity(old_entity).despawn();
        }

        // Get the chunk data
        let Some(chunk) = chunk_provider.get_chunk(chunk_pos) else {
            continue;
        };

        // Create new collider bundle
        if let Some(bundle) = ChunkColliderBundle::new(&chunk, chunk_pos) {
            let entity = commands.spawn(bundle).id();
            collider_entities.entities.insert(chunk_pos, entity);
        }
    }
}

/// System that despawns chunk collider entities when their containing column is unloaded.
///
/// Listens for `ColUnloadEvent` and removes all collider entities for chunks in that column.
pub fn despawn_colliders_for_unloaded_columns(
    mut commands: Commands,
    mut events: MessageReader<ColUnloadEvent>,
    mut collider_entities: ResMut<ChunkColliderEntityMap>,
) {
    for event in events.read() {
        for chunk_pos in chunks_in_col(&event.0) {
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
        app.init_resource::<ChunkColliderEntityMap>()
            .add_message::<RebuildChunkColliderRequest>()
            .add_systems(Update, handle_chunk_collider_rebuild_requests::<P>)
            .add_systems(Update, despawn_colliders_for_unloaded_columns);
    }
}
