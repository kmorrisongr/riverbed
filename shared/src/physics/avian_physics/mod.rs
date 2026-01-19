//! Avian3d-based physics simulation for voxel worlds using chunk trimesh colliders.
//!
//! Key ideas:
//! - Dynamic player bodies with capsule colliders and custom kinematics
//! - Static chunk colliders built from voxel meshes
//! - Shared movement code for client prediction and server authority

mod bundle;
mod movement;
mod plugin;

pub use bundle::DynamicPlayerPhysicsBundle;
pub use movement::{
    apply_player_input_to_physics, compute_player_desired_velocity,
    compute_velocity_from_player_actions, player_actions_to_movement_input, MovementInput,
    MovementMode, MovementStepResult, AIR_FRICTION, GROUND_ACCELERATION, GROUND_FRICTION,
    PLAYER_CAPSULE_HEIGHT, PLAYER_CAPSULE_RADIUS, PLAYER_GRAVITY, PLAYER_JUMP_VELOCITY,
    PLAYER_QUERY_BOUNDS,
};
pub use plugin::SharedPhysicsWorldPlugin;
