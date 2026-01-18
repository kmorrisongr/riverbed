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
    actions_to_camera_relative_input, apply_movement_step_to_components, compute_desired_velocity,
    compute_movement_step_from_actions, MovementInput, MovementMode, MovementStepResult,
    AIR_FRICTION, GROUND_ACCELERATION, GROUND_FRICTION, PLAYER_CAPSULE_HEIGHT,
    PLAYER_CAPSULE_RADIUS, PLAYER_GRAVITY, PLAYER_JUMP_FORCE, PLAYER_QUERY_BOUNDS,
};
pub use plugin::SharedPhysicsWorldPlugin;
