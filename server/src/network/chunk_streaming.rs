//! Chunk streaming via renet, coexisting alongside lightyear.
//!
//! This module provides chunk data streaming from server to clients using renet.
//! It tracks which chunks have been sent to each client and broadcasts new/changed
//! chunks as players move through the world.

use bevy::prelude::*;
use bevy_renet::renet::{ClientId, RenetServer};
use crossbeam::channel::Receiver;
use shared::meshing::ChunkColliderRebuildRequest;
use shared::net::{chunk_message_to_payload, ChunkDataMessage, STC_CHUNK_DATA_CHANNEL};
use shared::physics::Position;
use shared::world::chunk::Chunk;
use shared::world::pos::pos2d::{chunks_in_col, ColPos};
use shared::world::pos::pos3d::ChunkPos;
use shared::world::realm::Realm;
use std::collections::{HashMap, HashSet};

use crate::world::voxel_world::VoxelWorld;

const MAX_CHUNKS_PER_CLIENT_PER_TICK: usize = 16;

#[derive(Resource, Default)]
pub struct ChunkStreamingTick(pub u64);

/// Tracks which chunks have been sent to each connected client.
///
/// This prevents redundantly sending the same chunk data multiple times.
/// When a chunk changes (e.g., block placed/broken), it's invalidated so
/// it will be re-sent on the next broadcast cycle.
#[derive(Resource, Default)]
pub struct ChunkDeliveryTracker {
    /// Maps each client to the set of chunk positions they've received.
    chunks_delivered_to_client: HashMap<ClientId, HashSet<ChunkPos>>,
}

impl ChunkDeliveryTracker {
    pub fn mark_delivered(&mut self, client_id: ClientId, chunk_position: ChunkPos) {
        self.chunks_delivered_to_client
            .entry(client_id)
            .or_default()
            .insert(chunk_position);
    }

    pub fn was_delivered(&self, client_id: ClientId, chunk_position: &ChunkPos) -> bool {
        self.chunks_delivered_to_client
            .get(&client_id)
            .map(|chunks| chunks.contains(chunk_position))
            .unwrap_or(false)
    }

    pub fn invalidate_chunk(&mut self, chunk_position: &ChunkPos) {
        for chunks in self.chunks_delivered_to_client.values_mut() {
            chunks.remove(chunk_position);
        }
    }

    pub fn remove_client(&mut self, client_id: ClientId) {
        self.chunks_delivered_to_client.remove(&client_id);
    }
}

#[derive(Resource)]
pub struct ChunkChangesReceiver(pub Receiver<ChunkPos>);

/// System to process chunk changes from the VoxelWorld and invalidate delivery tracking.
pub fn process_chunk_changes(
    chunk_changes: Option<Res<ChunkChangesReceiver>>,
    mut tracker: ResMut<ChunkDeliveryTracker>,
    mut collider_rebuild_requests: MessageWriter<ChunkColliderRebuildRequest>,
) {
    let Some(chunk_changes) = chunk_changes else {
        return;
    };

    while let Ok(chunk_position) = chunk_changes.0.try_recv() {
        tracker.invalidate_chunk(&chunk_position);
        collider_rebuild_requests.write(ChunkColliderRebuildRequest::new(chunk_position));
    }
}

/// Marker component for entities tracked by the chunk streaming system.
/// This is added to player character entities to track their position for streaming.
#[derive(Component)]
pub struct ChunkStreamingPlayer {
    pub client_id: ClientId,
}

/// System to broadcast chunk data to connected clients based on their position.
pub fn broadcast_chunk_data(
    mut server: ResMut<RenetServer>,
    mut tick: ResMut<ChunkStreamingTick>,
    world: Res<VoxelWorld>,
    mut tracker: ResMut<ChunkDeliveryTracker>,
    player_query: Query<(&ChunkStreamingPlayer, &Position, &Realm)>,
) {
    tick.0 += 1;
    let render_distance = world.render_distance as i32;

    for client_id in server.clients_id() {
        // Find the player entity for this client
        let Some((_, position, realm)) = player_query
            .iter()
            .find(|(csp, _, _)| csp.client_id == client_id)
        else {
            continue;
        };

        let player_position = position.0;

        let player_chunk_column = ColPos {
            x: (player_position.x / 16.0).floor() as i32,
            z: (player_position.z / 16.0).floor() as i32,
            realm: *realm,
        };

        let mut chunks_to_send: HashMap<ChunkPos, Chunk> = HashMap::new();

        'outer: for dx in -render_distance..=render_distance {
            for dz in -render_distance..=render_distance {
                let column = ColPos {
                    x: player_chunk_column.x + dx,
                    z: player_chunk_column.z + dz,
                    realm: player_chunk_column.realm,
                };

                for chunk_position in chunks_in_col(&column) {
                    if tracker.was_delivered(client_id, &chunk_position) {
                        continue;
                    }

                    let Some(chunk_entry) = world.chunks.get(&chunk_position) else {
                        continue;
                    };

                    let chunk = chunk_entry.value().read().clone();
                    chunks_to_send.insert(chunk_position, chunk);
                    tracker.mark_delivered(client_id, chunk_position);

                    if chunks_to_send.len() >= MAX_CHUNKS_PER_CLIENT_PER_TICK {
                        break 'outer;
                    }
                }
            }
        }

        if chunks_to_send.is_empty() {
            continue;
        }

        let message = ChunkDataMessage {
            tick: tick.0,
            chunks: chunks_to_send,
        };

        debug!(
            "Sending {} chunks to client {}",
            message.chunks.len(),
            client_id
        );

        let payload = chunk_message_to_payload(&message);
        server.send_message(client_id, STC_CHUNK_DATA_CHANNEL, payload);
    }
}

/// Plugin that sets up chunk streaming via renet.
pub struct ChunkStreamingPlugin;

impl Plugin for ChunkStreamingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkDeliveryTracker>()
            .init_resource::<ChunkStreamingTick>()
            .add_systems(
                Update,
                (process_chunk_changes, broadcast_chunk_data).chain(),
            );
    }
}
