use crate::agents::PlayerControlled;
use bevy::prelude::*;
use crossbeam::channel::Sender;
use crossbeam_skiplist::SkipMap;
use lightyear::prelude::client::Client;
use lightyear::prelude::{MessageReceiver, MessageSender};
use parking_lot::RwLock;
use shared::{
    block::Block,
    net::lightyear_protocol::{
        BlockChangeConfirm, BlockInteractionChannel, BlockInteractionRequest,
    },
    world::{
        chunk::Chunk,
        pos::{
            pos2d::ColPos,
            pos3d::{BlockPos, ChunkPos, ChunkedPos},
            PlayerCol,
        },
        unload_column, BlockAccess, ColumnUnloader, MAX_HEIGHT, Y_CHUNKS,
    },
};
use std::sync::Arc;

#[derive(Resource)]
pub struct RenderDistance {
    pub distance: i32,
}

impl Default for RenderDistance {
    fn default() -> Self {
        Self { distance: 32 }
    }
}

#[derive(Resource, Clone)]
pub struct ClientWorldMap {
    pub chunks: Arc<SkipMap<ChunkPos, RwLock<Arc<Chunk>>>>,
    chunk_changes: Sender<ChunkPos>,
}

impl ClientWorldMap {
    pub fn new(chunk_changes: Sender<ChunkPos>) -> Self {
        ClientWorldMap {
            chunks: Arc::new(SkipMap::new()),
            chunk_changes,
        }
    }

    /// Insert a chunk received from the server into the world map.
    pub fn insert_chunk(&self, chunk_pos: ChunkPos, chunk: Chunk) {
        self.chunks.insert(chunk_pos, RwLock::new(Arc::new(chunk)));
        let _ = self.chunk_changes.send(chunk_pos);
    }

    pub fn get_block(&self, pos: BlockPos) -> Block {
        let (chunk_pos, chunked_pos) = <(ChunkPos, _)>::from(pos);
        match self.chunks.get(&chunk_pos) {
            None => Block::Air,
            Some(chunk) => *chunk.value().read().get(chunked_pos),
        }
    }

    pub fn get_chunk_arc(&self, pos: ChunkPos) -> Option<Arc<Chunk>> {
        self.chunks
            .get(&pos)
            .map(|c| Arc::clone(&*c.value().read()))
    }

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

    pub fn mark_chunk_changed(&self, chunk_pos: ChunkPos) {
        let _ = self.chunk_changes.send(chunk_pos);
    }

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
        self.get_chunk_arc(pos)
    }
}

#[derive(Message, Debug, Clone)]
pub struct SetBlockRequest {
    pub pos: BlockPos,
    pub block: Block,
}

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
            .add_plugins(shared::meshing::ChunkColliderPlugin::<ClientWorldMap>::default())
            .add_systems(Update, process_block_requests)
            .add_systems(Update, handle_server_block_confirmations)
            .add_systems(Update, unload_distant_columns);
    }
}

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

fn process_block_requests(
    world_map: Option<Res<ClientWorldMap>>,
    mut requests: MessageReader<SetBlockRequest>,
    mut block_changed: MessageWriter<BlockChanged>,
    mut client_query: Query<&mut MessageSender<BlockInteractionRequest>, With<Client>>,
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

        // Send block change request to server via lightyear
        if let Ok(mut sender) = client_query.single_mut() {
            sender.send::<BlockInteractionChannel>(BlockInteractionRequest {
                position: request.pos,
                new_block: request.block,
            });
        }
    }
}

/// Handle server-confirmed block changes received via Lightyear.
///
/// This system processes BlockChangeConfirm messages from the server, which are:
/// 1. Confirmations of changes this client made (already applied optimistically)
/// 2. Changes made by other players
/// 3. Server-side block changes (redstone, physics, game events)
///
/// For other players' changes or server changes, we apply them to the local world.
/// For our own changes, this serves as confirmation (we already applied them optimistically).
fn handle_server_block_confirmations(
    world_map: Option<Res<ClientWorldMap>>,
    mut client_query: Query<&mut MessageReceiver<BlockChangeConfirm>, With<Client>>,
    mut block_changed: MessageWriter<BlockChanged>,
) {
    let Some(world_map) = world_map else {
        return;
    };

    let Ok(mut receiver) = client_query.single_mut() else {
        return;
    };

    for confirm in receiver.receive() {
        let current_block = world_map.get_block(confirm.position);

        // If the block is already what the server says it should be, skip
        // (this handles our own optimistic updates that were correct)
        if current_block == confirm.new_block {
            continue;
        }

        // Apply the server's authoritative change
        let (chunk_pos, chunked_pos) = <(ChunkPos, ChunkedPos)>::from(confirm.position);
        if let Some(entry) = world_map.chunks.get(&chunk_pos) {
            let mut lock = entry.value().write();
            let mut new_chunk = (**lock).clone();
            new_chunk.set(chunked_pos, confirm.new_block);
            *lock = Arc::new(new_chunk);
            drop(lock);

            world_map.mark_chunk_changed(chunk_pos);

            // Emit BlockChanged event for other systems (sounds, particles, etc.)
            block_changed.write(BlockChanged {
                pos: confirm.position,
                old_block: confirm.old_block,
                new_block: confirm.new_block,
            });
        }
    }
}
