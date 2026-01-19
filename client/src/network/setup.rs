//! Network setup resources and systems for the client.
//!
//! This module provides:
//! - Resources for identifying the current player and target server
//! - System to launch a local embedded server when not connecting to external

use bevy::prelude::*;
use rand::Rng;
use shared::messages::PlayerId;
use shared::{GameServerConfig, RENDER_DISTANCE, SOCKET_BIND_ERROR};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::thread;
use std::time::Duration;

/// Resource to supply a player name before profile creation.
#[derive(Resource, Debug, Default)]
pub struct PlayerNameSupplied {
    pub name: String,
}

/// Resource tracking which world to load/connect to.
#[derive(Resource, Debug, Clone)]
pub struct SelectedWorld {
    pub name: Option<String>,
}

impl Default for SelectedWorld {
    fn default() -> Self {
        Self {
            name: Some("default".to_string()),
        }
    }
}

/// Server tick at the time of connection (for synchronization).
#[derive(Resource, Default, Debug, Clone)]
pub struct ServerTickAtConnect(pub u64);

/// World seed received from server.
#[derive(Resource, Default, Debug, Clone)]
pub struct WorldSeed(pub u32);

/// Current player's profile (ID and name).
#[derive(Resource, Clone)]
pub struct CurrentPlayerProfile {
    pub id: PlayerId,
    pub name: String,
}

impl CurrentPlayerProfile {
    pub(crate) fn new() -> Self {
        let mut rng = rand::rng();
        let id: u64 = rng.random();
        Self {
            id,
            name: format!("Player-{id}"),
        }
    }
}

fn hash_string_to_u64(input: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

impl FromWorld for CurrentPlayerProfile {
    fn from_world(world: &mut World) -> Self {
        let player_name = world.get_resource::<PlayerNameSupplied>();
        match player_name {
            Some(player_name) => Self {
                id: hash_string_to_u64(&player_name.name),
                name: player_name.name.clone(),
            },
            None => CurrentPlayerProfile::new(),
        }
    }
}

/// Target server configuration.
#[derive(Resource, Debug, Clone, Default)]
pub struct TargetServer {
    pub address: Option<SocketAddr>,
    pub username: Option<String>,
}

/// Startup system to launch a local embedded server if no external server address is set.
pub fn launch_local_server_system(
    mut target: ResMut<TargetServer>,
    selected_world: Res<SelectedWorld>,
) {
    if target.address.is_some() {
        debug!("Skipping launch local server - address already set");
        return;
    }

    if let Some(world_name) = &selected_world.name {
        info!("Launching local server with world: {}", world_name);

        let socket = match server::acquire_local_ephemeral_udp_socket(IpAddr::V4(Ipv4Addr::new(
            127, 0, 0, 1,
        ))) {
            Ok(socket) => socket,
            Err(err) => {
                error!("{}: {err}", SOCKET_BIND_ERROR);
                return;
            }
        };

        let address = match socket.local_addr() {
            Ok(address) => address,
            Err(err) => {
                error!("Failed to get socket local address: {err}");
                return;
            }
        };
        info!("Local server will bind to: {}", address);

        let world_name_clone = world_name.clone();

        thread::spawn(move || {
            server::init(
                socket,
                GameServerConfig {
                    world_name: world_name_clone,
                    is_solo: true,
                    broadcast_render_distance: RENDER_DISTANCE,
                },
            );
        });

        thread::sleep(Duration::from_millis(100));

        target.address = Some(address);
        info!("Local server launched, client will connect to {}", address);
    } else {
        error!("Error: No world selected. Unable to launch the server.");
    }
}
