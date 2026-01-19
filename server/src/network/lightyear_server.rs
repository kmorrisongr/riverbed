//! Lightyear server bootstrap: spins up a netcode UDP listener and registers
//! our replication protocol. Feature-gated behind `lightyear-net` so Renet
//! remains the default path while this matures.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use bevy::log::info;
use bevy::input::InputPlugin;
use bevy::prelude::*;
use bevy::transform::TransformPlugin;
use lightyear::netcode::{NetcodeServer, PRIVATE_KEY_BYTES};
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use shared::messages::ActionMask;
use shared::net::lightyear_protocol::LightyearProtocolPlugin;
use shared::{PROTOCOL_ID, TICKS_PER_SECOND};

const LIGHTYEAR_DEV_PRIVATE_KEY: [u8; PRIVATE_KEY_BYTES] = [0; PRIVATE_KEY_BYTES];

/// Server networking settings for the Lightyear path.
#[derive(Resource, Debug, Clone)]
pub struct LightyearServerConfig {
    pub bind_addr: SocketAddr,
    pub protocol_id: u64,
    pub private_key: [u8; PRIVATE_KEY_BYTES],
    pub max_clients: usize,
    pub keep_alive_hz: f64,
    pub client_timeout_secs: i32,
    pub num_disconnect_packets: usize,
}

impl Default for LightyearServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 5000),
            protocol_id: PROTOCOL_ID,
            private_key: LIGHTYEAR_DEV_PRIVATE_KEY,
            max_clients: 64,
            keep_alive_hz: 10.0,
            client_timeout_secs: 10,
            num_disconnect_packets: 10,
        }
    }
}

/// Latest inputs received from clients (placeholder until Lightyear transport is wired).
#[derive(Resource, Default, Debug, Clone)]
pub struct LightyearServerInbox {
    pub pending: Vec<(PeerId, u64, ActionMask)>,
}

pub struct LightyearServerPlugin;

impl Plugin for LightyearServerPlugin {
    fn build(&self, app: &mut App) {
        // Bevy basics Lightyear expects but our headless stack may omit.
        if !app.is_plugin_added::<TransformPlugin>() {
            app.add_plugins(TransformPlugin);
        }
        if !app.is_plugin_added::<InputPlugin>() {
            app.add_plugins(InputPlugin);
        }

        // Core Lightyear server stack and our protocol registration.
        app.add_plugins(ServerPlugins {
            tick_duration: Duration::from_secs_f64(1.0 / TICKS_PER_SECOND as f64),
        });
        app.add_plugins(LightyearProtocolPlugin)
            .init_resource::<LightyearServerConfig>()
            .init_resource::<LightyearServerInbox>()
            .add_systems(Startup, spawn_lightyear_server);
    }
}

/// Spawn the netcode-backed Lightyear server and kick off the transport.
fn spawn_lightyear_server(mut commands: Commands, config: Res<LightyearServerConfig>) {
    // Only spin up the listener once even if the plugin is hot-reloaded.
    if commands
        .world_scope(|world| world.iter_entities().any(|e| e.contains::<NetcodeServer>()))
    {
        return;
    }

    let server_entity = commands
        .spawn((
            Name::new("Lightyear Netcode Server"),
            NetcodeServer::new(NetcodeConfig {
                protocol_id: config.protocol_id,
                private_key: config.private_key,
                keep_alive_send_rate: 1.0 / config.keep_alive_hz,
                client_timeout_secs: config.client_timeout_secs,
                num_disconnect_packets: config.num_disconnect_packets,
            }),
            LocalAddr(config.bind_addr),
            ServerUdpIo::default(),
        ))
        .id();

    // Start listening immediately.
    commands.trigger(Start {
        entity: server_entity,
    });

    info!("Lightyear server listening on {}", config.bind_addr);
}
