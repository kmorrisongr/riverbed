#![allow(dead_code)]
//! Lightyear client bootstrap: configures the client plugin stack, spawns a
//! netcode-backed connection, and keeps capturing inputs for later timeline
//! ingestion. Feature-gated behind `lightyear-net` so Renet remains default.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use bevy::log::info;
use bevy::prelude::*;
use bevy::transform::TransformPlugin;
use lightyear::netcode::{NetcodeClient, PRIVATE_KEY_BYTES};
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use leafwing_input_manager::prelude::ActionState;
use shared::messages::ActionMask;
use shared::net::lightyear_inputs::{action_mask_from_leafwing, PlayerInputAction};
use shared::net::lightyear_protocol::LightyearProtocolPlugin;
use shared::{PROTOCOL_ID, TICKS_PER_SECOND};

use crate::agents::PlayerControlled;
use crate::network::buffered_client::SyncTime;
use crate::network::setup::{CurrentPlayerProfile, TargetServer};
use crate::render::FpsCam;

const LIGHTYEAR_DEV_PRIVATE_KEY: [u8; PRIVATE_KEY_BYTES] = [0; PRIVATE_KEY_BYTES];

/// Networking settings for the Lightyear path on the client.
#[derive(Resource, Debug, Clone)]
pub struct LightyearClientConfig {
    pub server_addr: SocketAddr,
    pub client_addr: SocketAddr,
    pub protocol_id: u64,
    pub private_key: [u8; PRIVATE_KEY_BYTES],
    pub client_id: u64,
    pub keep_alive_hz: f64,
    pub client_timeout_secs: i32,
    pub token_expire_secs: i32,
    pub num_disconnect_packets: usize,
}

impl Default for LightyearClientConfig {
    fn default() -> Self {
        Self {
            server_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000),
            client_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
            protocol_id: PROTOCOL_ID,
            private_key: LIGHTYEAR_DEV_PRIVATE_KEY,
            client_id: 1,
            keep_alive_hz: 10.0,
            client_timeout_secs: 10,
            token_expire_secs: 30,
            num_disconnect_packets: 10,
        }
    }
}

/// Latest input we intend to feed into a Lightyear input timeline.
#[derive(Resource, Default, Debug, Clone)]
pub struct LightyearInputSnapshot {
    pub tick_ms: u64,
    pub action_mask: ActionMask,
    pub camera: Transform,
}

pub struct LightyearClientPlugin;

impl Plugin for LightyearClientPlugin {
    fn build(&self, app: &mut App) {
        // Ensure core plugins Lightyear expects are present (most come from DefaultPlugins already).
        if !app.is_plugin_added::<TransformPlugin>() {
            app.add_plugins(TransformPlugin);
        }

        // Core Lightyear client stack + our protocol.
        app.add_plugins(ClientPlugins {
            tick_duration: Duration::from_secs_f64(1.0 / TICKS_PER_SECOND as f64),
        });
        app.add_plugins(LightyearProtocolPlugin);

        // Resources and systems shared with gameplay.
        app.init_resource::<LightyearClientConfig>()
            .init_resource::<LightyearInputSnapshot>()
            .add_systems(Startup, spawn_lightyear_client)
            .add_systems(PreUpdate, capture_lightyear_inputs);
    }
}

/// Spawn a Lightyear netcode client and kick off the connection.
fn spawn_lightyear_client(
    mut commands: Commands,
    config: Res<LightyearClientConfig>,
    target_server: Option<Res<TargetServer>>,
    player_profile: Option<Res<CurrentPlayerProfile>>,
) {
    // Only create one Lightyear client instance.
    let already_spawned = commands.world_scope(|world| {
        world
            .iter_entities()
            .any(|e| e.contains::<NetcodeClient>())
    });
    if already_spawned {
        return;
    }

    let server_addr = target_server
        .as_ref()
        .and_then(|t| t.address)
        .unwrap_or(config.server_addr);
    let client_id = player_profile
        .as_ref()
        .map(|p| p.id)
        .unwrap_or(config.client_id);

    let auth = Authentication::Manual {
        server_addr,
        client_id,
        private_key: config.private_key,
        protocol_id: config.protocol_id,
    };

    let netcode_config = NetcodeConfig {
        num_disconnect_packets: config.num_disconnect_packets,
        keepalive_packet_send_rate: 1.0 / config.keep_alive_hz,
        client_timeout_secs: config.client_timeout_secs,
        token_expire_secs: config.token_expire_secs,
    };

    let client_entity = commands
        .spawn((
            Name::new("Lightyear Netcode Client"),
            Client::default(),
            Link::new(None),
            LocalAddr(config.client_addr),
            PeerAddr(server_addr),
            ReplicationReceiver::default(),
            PredictionManager::default(),
            NetcodeClient::new(auth, netcode_config).expect("failed to build netcode client"),
            UdpIo::default(),
        ))
        .id();

    commands.trigger(Connect {
        entity: client_entity,
    });

    info!("Lightyear client connecting to {} as {}", server_addr, client_id);
}

fn capture_lightyear_inputs(
    actions: Query<&ActionState<PlayerInputAction>, With<PlayerControlled>>,
    camera: Query<&Transform, With<FpsCam>>,
    sync_time: Option<Res<SyncTime>>,
    mut snapshot: ResMut<LightyearInputSnapshot>,
) {
    let Ok(action_state) = actions.get_single() else {
        return;
    };

    let camera_transform = camera.get_single().copied().unwrap_or_default();
    let tick_ms = sync_time.as_ref().map(|s| s.now_synced() as u64).unwrap_or(0);

    snapshot.tick_ms = tick_ms;
    snapshot.action_mask = action_mask_from_leafwing(action_state);
    snapshot.camera = camera_transform;
}
