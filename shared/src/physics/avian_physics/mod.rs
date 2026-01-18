//! Avian3d-based physics simulation for voxel worlds using chunk trimesh colliders.
//!
//! Key ideas:
//! - Dynamic player bodies with capsule colliders and custom kinematics
//! - Static chunk colliders built from voxel meshes
//! - Shared movement code for client prediction and server authority

mod bundle;
mod movement;
mod plugin;

pub use bundle::PlayerPhysicsBundle;
pub use movement::{
    actions_to_movement_input, apply_player_input_step, apply_player_input_to_components,
    compute_desired_velocity, MovementInput, MovementMode, PlayerStepOutput, AIR_FRICTION,
    GROUND_ACCELERATION, GROUND_FRICTION, PLAYER_CAPSULE_HEIGHT, PLAYER_CAPSULE_RADIUS,
    PLAYER_GRAVITY, PLAYER_JUMP_FORCE, PLAYER_QUERY_BOUNDS,
};
pub use plugin::SharedPhysicsPlugin;
