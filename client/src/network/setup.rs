use bevy::prelude::*;
use rand::Rng;
use shared::messages::PlayerId;
use shared::{GameServerConfig, RENDER_DISTANCE, SOCKET_BIND_ERROR};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::thread;
use std::time::Duration;

use super::chunk_streaming::ChunkClientConfig;

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

#[derive(Resource, Clone)]
pub struct CurrentPlayerProfile {
    pub id: PlayerId,
}

impl CurrentPlayerProfile {
    pub(crate) fn new() -> Self {
        let mut rng = rand::rng();
        let id: u64 = rng.random();
        Self { id }
    }
}

impl FromWorld for CurrentPlayerProfile {
    fn from_world(world: &mut World) -> Self {
        let _ = world; // silence unused warning while allowing FromWorld impl
        CurrentPlayerProfile::new()
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct TargetServer {
    pub address: Option<SocketAddr>,
}

pub fn launch_local_server_system(
    mut target: ResMut<TargetServer>,
    selected_world: Res<SelectedWorld>,
    mut client_cfg: Option<ResMut<crate::network::lightyear_client::LightyearClientConfig>>,
    mut chunk_client_cfg: Option<ResMut<ChunkClientConfig>>,
    player_profile: Res<CurrentPlayerProfile>,
) {
    if target.address.is_some() {
        debug!("Skipping launch local server - address already set");
        return;
    }

    if let Some(world_name) = &selected_world.name {
        info!("Launching local server with world: {}", world_name);

        let address = match server::acquire_local_ephemeral_udp_socket(IpAddr::V4(Ipv4Addr::new(
            127, 0, 0, 1,
        ))) {
            Ok(address) => address,
            Err(err) => {
                error!("{}: {err}", SOCKET_BIND_ERROR);
                return;
            }
        };
        info!("Local server will bind to: {}", address);

        let world_name_clone = world_name.clone();

        thread::spawn(move || {
            server::init(
                address,
                GameServerConfig {
                    world_name: world_name_clone,
                    is_solo: true,
                    broadcast_render_distance: RENDER_DISTANCE,
                },
            );
        });

        thread::sleep(Duration::from_millis(100));

        target.address = Some(address);
        if let Some(cfg) = client_cfg.as_deref_mut() {
            cfg.server_addr = address;
        }
        
        // Configure chunk streaming client to connect to the chunk server
        // The chunk server runs on port 5001 (same IP as lightyear server)
        if let Some(chunk_cfg) = chunk_client_cfg.as_deref_mut() {
            let chunk_server_addr = SocketAddr::new(address.ip(), 5001);
            chunk_cfg.server_addr = Some(chunk_server_addr);
            chunk_cfg.client_id = player_profile.id;
            info!("Chunk streaming client will connect to {}", chunk_server_addr);
        }
        
        info!("Local server launched, client will connect to {}", address);
    } else {
        error!("Error: No world selected. Unable to launch the server.");
    }
}
