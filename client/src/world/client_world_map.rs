//! Client-side world map that stores chunks and provides block access.
//!
//! This module provides a read-only view of the world for client systems,
//! with block modifications queued as events for network transmission.
//!
//! # Chunk Storage Strategy (Copy-on-Write)
//!
//! Chunks are stored as `RwLock<Arc<Chunk>>`. This enables efficient concurrent access:
//! - **Readers** (mesh thread, collider cooking) clone the `Arc<Chunk>` - O(1) atomic increment
//! - **Writers** (block placement) create a new `Arc<Chunk>` with modifications (COW)
//!
//! This avoids expensive full-chunk clones when passing data to worker threads.

use crate::agents::PlayerControlled;
use bevy::prelude::*;
use crossbeam::channel::Sender;
use crossbeam_skiplist::SkipMap;
use parking_lot::RwLock;
use shared::{
    block::Block,
    world::{
        chunk::Chunk,
        pos::{
            pos2d::ColPos,
            pos3d::{BlockPos, ChunkPos, ChunkedPos},
            PlayerCol,
        },
        ColumnUnloader, BlockAccess, MAX_HEIGHT, Y_CHUNKS, unload_column,
    },
};
use std::sync::Arc;

/// Client-side render distance configuration.
///
/// This controls how far chunks are rendered on the client side.
/// The server may send chunks for a larger area, but the client will only
/// render and keep in memory chunks within this distance.
#[derive(Resource)]
pub struct RenderDistance {
    /// Render distance in chunks
    pub distance: i32,
}

impl Default for RenderDistance {
    fn default() -> Self {
        Self { distance: 32 }
    }
}

/// Client-side world map resource.
///
/// Provides read-only block access for physics, raycasting, and rendering.
/// Block modifications are queued as events rather than applied directly.
///
/// Chunks are stored using a Copy-on-Write pattern: `RwLock<Arc<Chunk>>`.
/// This allows worker threads (mesh generation, collider cooking) to
/// efficiently obtain a reference-counted handle to chunk data without
/// cloning the entire chunk contents.
#[derive(Resource, Clone)]
pub struct ClientWorldMap {
    /// The chunk data, shared with worker threads via Arc cloning.
    /// Inner `Arc<Chunk>` is replaced (COW) when blocks are modified.
    pub chunks: Arc<SkipMap<ChunkPos, RwLock<Arc<Chunk>>>>,
    /// Channel to notify mesh thread of chunk changes
    chunk_changes: Sender<ChunkPos>,
}

impl ClientWorldMap {
    pub fn new(chunk_changes: Sender<ChunkPos>) -> Self {
        ClientWorldMap {
            chunks: Arc::new(SkipMap::new()),
            chunk_changes,
        }
    }

    /// Get a block at the given position
    pub fn get_block(&self, pos: BlockPos) -> Block {
        let (chunk_pos, chunked_pos) = <(ChunkPos, _)>::from(pos);
        match self.chunks.get(&chunk_pos) {
            None => Block::Air,
            Some(chunk) => *chunk.value().read().get(chunked_pos),
        }
    }

    /// Insert or update a chunk (called when receiving data from server)
    pub fn insert_chunk(&self, chunk_pos: ChunkPos, chunk: Chunk) {
        self.chunks.insert(chunk_pos, RwLock::new(Arc::new(chunk)));
        let _ = self.chunk_changes.send(chunk_pos);
    }

    /// Get a clone of the Arc<Chunk> for a given position.
    /// This is O(1) - just an atomic reference count increment.
    pub fn get_chunk_arc(&self, pos: ChunkPos) -> Option<Arc<Chunk>> {
        self.chunks.get(&pos).map(|c| Arc::clone(&*c.value().read()))
    }

    /// Unload all chunks in a column
    pub fn unload_col(&self, col: ColPos) {
        for y in 0..Y_CHUNKS as i32 {
            let chunk_pos = ChunkPos {
                x: col.x,
                y,
                z: col.z,
                realm: col.realm,
            };
            self.chunks.remove(&chunk_pos);
        }
    }

    /// Mark a chunk as changed (triggers mesh rebuild)
    pub fn mark_chunk_changed(&self, chunk_pos: ChunkPos) {
        let _ = self.chunk_changes.send(chunk_pos);
    }

    /// Get all unique column positions that have at least one chunk loaded
    pub fn loaded_columns(&self) -> Vec<ColPos> {
        use std::collections::HashSet;
        let mut cols: HashSet<ColPos> = HashSet::new();
        for entry in self.chunks.iter() {
            let chunk_pos = entry.key();
            cols.insert(ColPos {
                x: chunk_pos.x,
                z: chunk_pos.z,
                realm: chunk_pos.realm,
            });
        }
        cols.into_iter().collect()
    }
}

impl ColumnUnloader for ClientWorldMap {
    fn unload_column_impl(&self, col: ColPos) {
        self.unload_col(col);
    }
}

impl BlockAccess for ClientWorldMap {
    fn get_block_safe(&self, pos: BlockPos) -> Block {
        if pos.y < 0 || pos.y >= MAX_HEIGHT as i32 {
            Block::Air
        } else {
            self.get_block(pos)
        }
    }

    fn is_chunk_loaded(&self, chunk_pos: ChunkPos) -> bool {
        self.chunks.contains_key(&chunk_pos)
    }
}

impl shared::meshing::ChunkProvider for ClientWorldMap {
    fn get_chunk(&self, pos: ChunkPos) -> Option<Arc<Chunk>> {
        // Simply clone the Arc - no data copying needed!
        self.get_chunk_arc(pos)
    }
}

/// Event sent when requesting to set a block.
/// This will be picked up by the network system and sent to the server.
#[derive(Message, Debug, Clone)]
pub struct SetBlockRequest {
    pub pos: BlockPos,
    pub block: Block,
}

/// Event sent when the server confirms a block change.
/// Systems should listen to this to update local state.
#[derive(Message, Debug, Clone)]
pub struct BlockChanged {
    pub pos: BlockPos,
    pub old_block: Block,
    pub new_block: Block,
}

// Re-export ColUnloadEvent from shared for convenience
pub use shared::world::ColUnloadEvent;

/// Plugin to set up the client world system
pub struct ClientWorldPlugin;

impl Plugin for ClientWorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderDistance>()
            .add_message::<SetBlockRequest>()
            .add_message::<BlockChanged>()
            .add_message::<ColUnloadEvent>()
            // Add chunk collider plugin for physics
            .add_plugins(shared::meshing::ChunkColliderPlugin::<ClientWorldMap>::default())
            .add_systems(Update, process_block_requests)
            .add_systems(Update, unload_distant_columns);
    }
}

/// Unloads columns that are outside the client's render distance.
///
/// This system prevents memory from growing unbounded as the player travels.
/// The server may send chunks for a larger area than the client renders,
/// allowing clients to adjust render distance for their hardware performance.
fn unload_distant_columns(
    world_map: Option<Res<ClientWorldMap>>,
    render_distance: Res<RenderDistance>,
    player_query: Query<&PlayerCol, With<PlayerControlled>>,
    mut unload_events: MessageWriter<ColUnloadEvent>,
) {
    let Some(world_map) = world_map else {
        return;
    };

    let Ok(player_col) = player_query.single() else {
        return;
    };

    let player_pos = player_col.0;
    let distance = render_distance.distance;

    for col in world_map.loaded_columns() {
        // Check if column is outside render distance (using square distance for efficiency)
        let dx = (col.x - player_pos.x).abs();
        let dz = (col.z - player_pos.z).abs();

        // Also check realm - unload columns from different realms
        if col.realm != player_pos.realm || dx > distance || dz > distance {
            unload_column(&*world_map, col, &mut unload_events);
        }
    }
}

/// Process block requests: apply locally and send to server.
///
/// This uses client-side prediction - we apply the change immediately locally
/// for responsiveness, and also send it to the server. If the server rejects it,
/// the next WorldUpdate will correct our local state.
fn process_block_requests(
    world_map: Option<Res<ClientWorldMap>>,
    mut requests: MessageReader<SetBlockRequest>,
    mut block_changed: MessageWriter<BlockChanged>,
    mut client: Option<ResMut<bevy_renet::renet::RenetClient>>,
) {
    let Some(world_map) = world_map else {
        return;
    };

    for request in requests.read() {
        let old_block = world_map.get_block(request.pos);

        // Apply the change locally immediately (client-side prediction)
        // Uses Copy-on-Write: clone the chunk, modify, then swap the Arc
        let (chunk_pos, chunked_pos) = <(ChunkPos, ChunkedPos)>::from(request.pos);
        if let Some(entry) = world_map.chunks.get(&chunk_pos) {
            let mut lock = entry.value().write();
            // Clone the chunk data, modify it, then replace the Arc
            let mut new_chunk = (**lock).clone();
            new_chunk.set(chunked_pos, request.block);
            *lock = Arc::new(new_chunk);
            drop(lock);
            
            world_map.mark_chunk_changed(chunk_pos);

            block_changed.write(BlockChanged {
                pos: request.pos,
                old_block,
                new_block: request.block,
            });
        }

        // Send to server for authoritative processing
        if let Some(ref mut client) = client {
            use crate::network::SendGameMessageExtension;
            use shared::messages::{ClientToServerBlockInteraction, ClientToServerMessage};

            client.send_game_message(ClientToServerMessage::BlockInteraction(
                ClientToServerBlockInteraction {
                    position: request.pos,
                    new_block: request.block,
                },
            ));
        }
    }
}
