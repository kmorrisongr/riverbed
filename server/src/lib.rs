//! Server library crate
//!
//! This module exposes the server initialization function for use by the client
//! when running in singleplayer mode (local server).

mod generation;
mod init;
mod logging;
pub mod network;
pub mod world;

use std::net::{IpAddr, SocketAddr};

use bevy::log::info;
use bevy::prelude::*;
use network::lightyear_server::{LightyearServerConfig, LightyearServerPlugin};
use shared::GameServerConfig;

/// Acquires an available ephemeral socket address on the given IP.
/// Used by the client to pick a bind address for the local server.
pub fn acquire_local_ephemeral_udp_socket(ip: IpAddr) -> std::io::Result<SocketAddr> {
    let addr = SocketAddr::new(ip, 0); // Port 0 = ephemeral port
    let socket = std::net::UdpSocket::bind(addr)?;
    socket.local_addr()
}

/// Initialize and run the server with the given configuration.
///
/// This function blocks until the server shuts down.
/// It's designed to be called from a separate thread when running in singleplayer mode.
///
/// # Arguments
/// * `bind_addr` - Address for Lightyear to bind
/// * `config` - Server configuration including world name and settings
///
/// # Example
/// ```ignore
/// use std::thread;
/// use std::net::{IpAddr, Ipv4Addr};
///
/// let addr = server::acquire_local_ephemeral_udp_socket(
///     IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
/// ).unwrap();
///
/// thread::spawn(move || {
///     server::init(addr, GameServerConfig {
///         world_name: "my_world".to_string(),
///         is_solo: true,
///         broadcast_render_distance: 8,
///     });
/// });
/// ```
pub fn init(bind_addr: SocketAddr, config: GameServerConfig) {
    info!("Server starting on {}", bind_addr);

    let mut app = App::new();

    init::configure_server_app(&mut app, init::ServerInitConfig {
        game_config: config,
        add_log_plugin: false, // Client already has logging configured
    });

    app.insert_resource(LightyearServerConfig {
        bind_addr,
        ..Default::default()
    });

    app.add_plugins(LightyearServerPlugin);

    info!("Server entering main loop");
    app.run();
}
