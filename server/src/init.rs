//! Common server initialization logic shared between standalone and embedded modes.

use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::asset::AssetPlugin;
use bevy::log::info;
use bevy::prelude::*;
use bevy::scene::ScenePlugin;
use bevy::state::app::StatesPlugin;
use crossbeam::channel;
use shared::meshing::ChunkColliderPlugin;
use shared::physics::SharedPhysicsWorldPlugin;
use shared::world::pos::pos3d::ChunkPos;
use shared::world::world_rng::WorldRng;
use shared::world::WorldSeed;
use shared::{GameServerConfig, TICKS_PER_SECOND};

use crate::logging::make_log_channel;
use crate::world::voxel_world::VoxelWorld;
use crate::world::TerrainLoadPlugin;

/// Configuration options for server initialization
pub struct ServerInitConfig {
    /// The game server configuration (world name, render distance, etc.)
    pub game_config: GameServerConfig,
    /// Whether to add the log plugin (should be false when embedded in client)
    pub add_log_plugin: bool,
}

/// Adds all common server plugins and resources to the app.
///
/// This is the shared initialization logic used by both standalone and embedded modes.
/// The caller is responsible for creating the `App` and calling `app.run()`
pub fn configure_server_app(
    app: &mut App,
    config: ServerInitConfig,
) {
    let seed: u64 = 42; // TODO: Load from world save or generate randomly

    // Create chunk changes channel for VoxelWorld
    let (chunk_changes_tx, _chunk_changes_rx) = channel::unbounded::<ChunkPos>();
    let mut voxel_world = VoxelWorld::new(chunk_changes_tx);
    voxel_world.render_distance = config.game_config.broadcast_render_distance as u32;

    // Minimal plugins for headless server, plus asset loading for physics
    app.add_plugins(
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / TICKS_PER_SECOND as f64,
        ))),
    );

    app.add_plugins(AssetPlugin::default());
    app.add_plugins(StatesPlugin);
    app.add_plugins(ScenePlugin);
    app.init_asset::<Mesh>();

    // Optionally add log plugin (standalone server needs it, embedded doesn't)
    if config.add_log_plugin {
        use shared::logging::logging::RiverbedLogPlugin;
        app.add_plugins(RiverbedLogPlugin);
    }

    app.add_plugins(SharedPhysicsWorldPlugin);

    app.add_plugins(ChunkColliderPlugin::<VoxelWorld>::default());

    // Always insert LogEventSender (needed by terrain thread)
    let (sender, _receiver) = make_log_channel();
    app.insert_resource(sender);

    // Insert game resources
    app.insert_resource(config.game_config);
    app.insert_resource(voxel_world);
    app.insert_resource(WorldSeed(seed as u32));
    app.insert_resource(WorldRng::new(seed));

    // Add terrain loading plugin (handles terrain generation based on player positions)
    app.add_plugins(TerrainLoadPlugin);

    info!("Server initialized");
}
