//! Systems for synchronizing chunk colliders with the world.
//!
//! This module provides Bevy systems that automatically spawn and despawn
//! chunk collider entities as chunks are loaded and unloaded. It's designed
//! to work with both client and server, receiving chunk change notifications
//! via events.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::world::pos::pos2d::{chunks_in_col, ColPos};
use crate::world::pos::pos3d::ChunkPos;
use crate::world::chunk::Chunk;

use super::chunk_collider::ChunkColliderBundle;

/// Event indicating a chunk was added or modified and needs its collider updated.
#[derive(Message, Debug, Clone, Copy)]
pub struct ChunkColliderUpdate {
    pub chunk_pos: ChunkPos,
}

/// Event indicating a column was unloaded and its colliders should be removed.
#[derive(Message, Debug, Clone, Copy)]
pub struct ColColliderUnload {
    pub col_pos: ColPos,
}

/// Resource mapping chunk positions to their collider entities.
#[derive(Resource, Default)]
pub struct ChunkColliderEntities {
    pub entities: HashMap<ChunkPos, Entity>,
}

/// Trait for accessing chunk data. Implemented by both ClientWorldMap and VoxelWorld.
pub trait ChunkProvider: Send + Sync + 'static {
    /// Get a chunk if it exists, returning a clone for thread safety.
    fn get_chunk(&self, pos: ChunkPos) -> Option<Chunk>;
}

/// System that spawns or updates chunk collider entities when chunks change.
pub fn update_chunk_colliders<P: ChunkProvider + Resource>(
    mut commands: Commands,
    mut events: MessageReader<ChunkColliderUpdate>,
    chunk_provider: Option<Res<P>>,
    mut collider_entities: ResMut<ChunkColliderEntities>,
) {
    let Some(chunk_provider) = chunk_provider else { return };
    
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

/// System that removes chunk colliders when columns are unloaded.
pub fn remove_column_colliders(
    mut commands: Commands,
    mut events: MessageReader<ColColliderUnload>,
    mut collider_entities: ResMut<ChunkColliderEntities>,
) {
    for event in events.read() {
        for chunk_pos in chunks_in_col(&event.col_pos) {
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
pub struct ChunkColliderPlugin<P: ChunkProvider + Resource> {
    _marker: std::marker::PhantomData<P>,
}

impl<P: ChunkProvider + Resource> Default for ChunkColliderPlugin<P> {
    fn default() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<P: ChunkProvider + Resource> ChunkColliderPlugin<P> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<P: ChunkProvider + Resource> Plugin for ChunkColliderPlugin<P> {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkColliderEntities>()
            .add_message::<ChunkColliderUpdate>()
            .add_message::<ColColliderUnload>()
            .add_systems(Update, update_chunk_colliders::<P>)
            .add_systems(Update, remove_column_colliders);
    }
}
