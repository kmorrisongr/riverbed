pub mod asset_processing;
pub mod block;
pub mod items;
pub mod logging;
pub mod meshing;
pub mod messages;
pub mod net;
pub mod physics;
pub mod utils;
pub mod world;

pub use block::{Block, BlockFamily};

use bevy::prelude::*;

// Network protocol constants
pub const PROTOCOL_ID: u64 = 0;
pub const TICKS_PER_SECOND: u64 = 20;
pub const RENDER_DISTANCE: i32 = 32;

// Player movement constants
pub const WALK_SPEED: f32 = 7.0;
pub const FLY_SPEED: f32 = 500.0;
pub const FLY_VERTICAL_SPEED: f32 = 100.0;

// Player spawn position (shared between client and server)
pub const DEFAULT_SPAWN_POSITION: Vec3 = Vec3::new(280., 500., -150.);

// Note: Physics constants are available via shared::physics::{PLAYER_GRAVITY, PLAYER_JUMP_VELOCITY, etc.}
// Re-export only the most commonly referenced query bounds for convenience
pub use physics::PLAYER_QUERY_BOUNDS;

// Error message constants
pub const UNIX_EPOCH_TIME_ERROR: &str = "System time is before UNIX_EPOCH";
pub const SOCKET_LOCAL_ADDR_ERROR: &str = "Failed to retrieve local address for UDP socket";
pub const SOCKET_BIND_ERROR: &str = "Failed to bind UDP socket";
pub const TARGET_SERVER_ADDR_ERROR: &str =
    "Target server address missing when initializing connection";
pub const NETCODE_CLIENT_TRANSPORT_ERROR: &str = "Failed to create Netcode client transport";
pub const NETCODE_SERVER_TRANSPORT_ERROR: &str = "Failed to create Netcode server transport";
pub const USERNAME_MISSING_AUTHENTICATED_ERROR: &str =
    "Username missing while handling authenticated session token";

#[derive(Resource)]
pub struct GameServerConfig {
    pub world_name: String,
    pub is_solo: bool,
    pub broadcast_render_distance: i32,
}

