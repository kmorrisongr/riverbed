use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use bevy::log::info;
use bevy::prelude::*;
use clap::Parser;
use server::network::lightyear_server::{LightyearServerConfig, LightyearServerPlugin};
use shared::{GameServerConfig, RENDER_DISTANCE};

mod generation;
mod init;
mod logging;
pub mod world;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 8000)]
    port: u16,

    #[arg(short, long, default_value = "default")]
    world: String,

    #[arg(short, long, default_value_t = RENDER_DISTANCE)]
    render_distance: i32,
}

fn main() {
    let args = Args::parse();

    // Validate render_distance
    if args.render_distance < 1 || args.render_distance > 32 {
        eprintln!("Error: render_distance must be between 1 and 32");
        std::process::exit(1);
    }

    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), args.port);
    info!("Server starting on {}", bind_addr);

    let mut app = App::new();

    init::configure_server_app(&mut app, init::ServerInitConfig {
        game_config: GameServerConfig {
            world_name: args.world,
            is_solo: false,
            broadcast_render_distance: args.render_distance,
        },
        add_log_plugin: true, // Standalone server needs its own logging
    });

    app.insert_resource(LightyearServerConfig {
        bind_addr,
        ..Default::default()
    });

    app.add_plugins(LightyearServerPlugin);

    info!("Server entering main loop");
    app.run();
}
