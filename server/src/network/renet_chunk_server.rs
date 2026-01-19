//! Renet server setup for chunk streaming.
//!
//! This creates a separate renet server for chunk data transmission,
//! running alongside the lightyear server which handles player replication.

use std::net::{SocketAddr, UdpSocket};
use std::time::SystemTime;

use bevy::prelude::*;
use bevy_renet::netcode::{
    NetcodeServerPlugin, NetcodeServerTransport, ServerAuthentication, ServerConfig,
};
use bevy_renet::renet::{RenetServer, ServerEvent};
use bevy_renet::RenetServerPlugin;
use lightyear::connection::client::Connected;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::RemoteId;
use shared::net::get_chunk_renet_config;
use shared::PROTOCOL_ID;

use super::chunk_streaming::ChunkDeliveryTracker;

/// Resource containing the renet chunk server's address.
#[derive(Resource, Debug, Clone)]
pub struct ChunkServerConfig {
    pub bind_addr: SocketAddr,
}

impl Default for ChunkServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:5001".parse().unwrap(),
        }
    }
}

/// Plugin that sets up the renet server for chunk streaming.
pub struct RenetChunkServerPlugin;

impl Plugin for RenetChunkServerPlugin {
    fn build(&self, app: &mut App) {
        // Add renet plugins
        app.add_plugins(RenetServerPlugin);
        app.add_plugins(NetcodeServerPlugin);

        app.init_resource::<ChunkServerConfig>();

        app.add_systems(Startup, setup_chunk_server);
        app.add_systems(Update, handle_chunk_server_events);
        app.add_observer(on_lightyear_client_connected);
    }
}

fn setup_chunk_server(mut commands: Commands, config: Res<ChunkServerConfig>) {
    let socket = match UdpSocket::bind(config.bind_addr) {
        Ok(s) => s,
        Err(e) => {
            error!(
                "Failed to bind chunk server socket on {}: {}",
                config.bind_addr, e
            );
            return;
        }
    };

    let local_addr = socket.local_addr().unwrap_or(config.bind_addr);

    let current_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("System time before UNIX epoch");

    let server_config = ServerConfig {
        current_time,
        max_clients: 64,
        protocol_id: PROTOCOL_ID,
        public_addresses: vec![local_addr],
        authentication: ServerAuthentication::Unsecure,
    };

    let transport = match NetcodeServerTransport::new(server_config, socket) {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to create chunk server transport: {}", e);
            return;
        }
    };

    let server = RenetServer::new(get_chunk_renet_config());

    commands.insert_resource(server);
    commands.insert_resource(transport);

    info!("Chunk streaming server listening on {}", local_addr);
}

fn handle_chunk_server_events(
    mut server_events: MessageReader<ServerEvent>,
    mut tracker: ResMut<ChunkDeliveryTracker>,
) {
    for event in server_events.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Chunk streaming: client {} connected", client_id);
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                info!(
                    "Chunk streaming: client {} disconnected: {}",
                    client_id, reason
                );
                tracker.remove_client(*client_id);
            }
        }
    }
}

/// When a lightyear client connects, add the ChunkStreamingPlayer component
/// to their character entity so the chunk streaming system can track them.
fn on_lightyear_client_connected(
    trigger: On<Add, Connected>,
    query: Query<&RemoteId, With<ClientOf>>,
    // We need to find character entities that are controlled by this client
    // The character is spawned in handle_connected in lightyear_server.rs
) {
    let Ok(remote_id) = query.get(trigger.entity) else {
        return;
    };

    // The lightyear client ID becomes the renet client ID for chunk streaming.
    // Note: This assumes the client connects to the chunk server with the same ID.
    let client_id = remote_id.0;

    info!(
        "Lightyear client {} connected, will track for chunk streaming",
        client_id
    );

    // The ChunkStreamingPlayer component will be added when the character spawns.
    // We store the mapping here for later use.
    // For now, we'll handle this in a separate system that watches for new characters.
}
