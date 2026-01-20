//! Client-side chunk streaming via renet.
//!
//! This module handles receiving chunk data from the server's renet chunk streaming
//! server and inserting chunks into the ClientWorldMap.

use std::net::{SocketAddr, UdpSocket};
use std::time::SystemTime;

use bevy::prelude::*;
use bevy_renet::netcode::{ClientAuthentication, NetcodeClientPlugin, NetcodeClientTransport};
use bevy_renet::renet::RenetClient;
use bevy_renet::RenetClientPlugin;
use shared::meshing::ChunkColliderRebuildRequest;
use shared::net::{get_chunk_renet_config, payload_to_chunk_message, ChunkDataMessage};
use shared::PROTOCOL_ID;

use crate::render::MeshOrderSender;
use crate::world::ClientWorldMap;

/// Configuration for the chunk streaming client
#[derive(Resource, Debug, Clone)]
pub struct ChunkClientConfig {
    /// The address of the chunk streaming server
    pub server_addr: Option<SocketAddr>,
    /// Client ID (should match lightyear client ID)
    pub client_id: u64,
}

impl Default for ChunkClientConfig {
    fn default() -> Self {
        Self {
            server_addr: None,
            client_id: 1,
        }
    }
}

/// Plugin for receiving chunk data from the server
pub struct ChunkStreamingClientPlugin;

impl Plugin for ChunkStreamingClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RenetClientPlugin);
        app.add_plugins(NetcodeClientPlugin);

        app.init_resource::<ChunkClientConfig>();

        app.add_systems(Update, try_connect_to_chunk_server);
        app.add_systems(Update, receive_chunk_data);
    }
}

/// Try to connect to the chunk streaming server once we have the address
fn try_connect_to_chunk_server(
    mut commands: Commands,
    config: Res<ChunkClientConfig>,
    existing_client: Option<Res<RenetClient>>,
) {
    // Already connected or connecting
    if existing_client.is_some() {
        return;
    }

    // No server address yet
    let Some(server_addr) = config.server_addr else {
        return;
    };

    info!(
        "Connecting to chunk streaming server at {} as client {}",
        server_addr, config.client_id
    );

    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to bind chunk client socket: {}", e);
            return;
        }
    };

    let current_time = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(t) => t,
        Err(e) => {
            error!("System time error: {}", e);
            return;
        }
    };

    let authentication = ClientAuthentication::Unsecure {
        server_addr,
        client_id: config.client_id,
        user_data: None,
        protocol_id: PROTOCOL_ID,
    };

    let transport = match NetcodeClientTransport::new(current_time, authentication, socket) {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to create chunk client transport: {}", e);
            return;
        }
    };

    let client = RenetClient::new(get_chunk_renet_config());

    commands.insert_resource(client);
    commands.insert_resource(transport);

    info!("Chunk streaming client initialized");
}

/// Receive chunk data from the server and add to the world map
fn receive_chunk_data(
    mut client: Option<ResMut<RenetClient>>,
    world_map: Option<Res<ClientWorldMap>>,
    mesh_order_sender: Option<Res<MeshOrderSender>>,
    mut ev_collider_rebuild: MessageWriter<ChunkColliderRebuildRequest>,
) {
    let Some(ref mut client) = client else {
        return;
    };

    let Some(world_map) = world_map else {
        return;
    };

    // Receive messages from channel 0 (STC_CHUNK_DATA_CHANNEL)
    while let Some(payload) = client.receive_message(0) {
        match payload_to_chunk_message::<ChunkDataMessage>(&payload) {
            Ok(message) => {
                let chunk_count = message.chunks.len();
                if chunk_count > 0 {
                    for (chunk_position, chunk) in message.chunks {
                        world_map.insert_chunk(chunk_position, chunk);

                        if let Some(ref mesh_sender) = mesh_order_sender {
                            if mesh_sender.0.send(chunk_position).is_err() {
                                warn!("Failed to send mesh order for chunk {:?}", chunk_position);
                            }
                        }

                        ev_collider_rebuild.write(ChunkColliderRebuildRequest::new(chunk_position));
                    }

                    debug!("Received and processed {} chunks from server", chunk_count);
                }
            }
            Err(e) => {
                warn!("Failed to deserialize chunk data: {:?}", e);
            }
        }
    }
}
