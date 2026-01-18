//! Shared physics simulation module using avian3d.
//!
//! This module provides physics types and velocity computation that can be used
//! by both client and server. The architecture is:
//!
//! - **Players**: `RigidBody::Dynamic` with capsule colliders
//! - **Chunks**: `RigidBody::Static` with trimesh colliders
//! - **Movement**: Custom kinematics (we compute velocity, avian3d resolves collisions)
//!
//! The server is authoritative for player positions. The client uses the same
//! velocity computation for prediction.

pub mod avian_physics;
pub mod ground_detection;
pub mod player_step;

// Re-export avian3d types for convenience
pub use avian3d::prelude::{
    AngularVelocity, Collider, Friction, GravityScale, LinearVelocity, LockedAxes, Position,
    Restitution, RigidBody, Rotation, SweptCcd,
};

// Re-export core physics types and functions from the avian integration
pub use avian_physics::{
    actions_to_movement_input, compute_desired_velocity, get_stepped_block,
    MovementInput, MovementMode, PhysicsState, PhysicsStepResult, PlayerPhysicsBundle,
    SharedPhysicsPlugin, AIR_FRICTION, GROUND_ACCELERATION, GROUND_FRICTION,
    PLAYER_CAPSULE_HEIGHT, PLAYER_CAPSULE_RADIUS, PLAYER_GRAVITY, PLAYER_JUMP_FORCE,
    PLAYER_QUERY_BOUNDS,
};

// Re-export ground detection from avian3d contacts
pub use ground_detection::{
    is_on_ground_from_contacts, update_ground_state_system, update_stepped_block_system,
    OnGround, SteppingOn, GROUND_NORMAL_THRESHOLD,
};

// Helpers used by both client and server when applying player inputs
pub use player_step::apply_player_input_to_components;

