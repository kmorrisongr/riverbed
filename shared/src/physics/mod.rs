//! Shared physics simulation module using avian3d.
//!
//! This module provides physics types and velocity computation that can be used
//! by both client and server. The architecture is:
//!
//! - **Players**: `RigidBody::Dynamic` with capsule colliders
//! - **Chunks**: `RigidBody::Static` with trimesh colliders (see `meshing::ChunkColliderPlugin`)
//! - **Movement**: Custom kinematics (we compute velocity, avian3d resolves collisions)
//!
//! # Server-Authoritative Model
//!
//! The server is the single source of truth for player state:
//! 1. Client captures inputs and sends to server
//! 2. Client predicts movement locally using `apply_movement_step_to_components()`
//! 3. Server processes inputs using the same function for identical velocity calculation
//! 4. Server broadcasts authoritative position updates
//! 5. Client reconciles prediction with server state (see `client::network::reconciliation`)
//!
//! # Key Functions
//!
//! - [`apply_movement_step_to_components`]: Applies input to ECS components (used by both client & server)
//! - [`compute_desired_velocity`]: Pure function for velocity calculation
//! - [`update_ground_state_system`]: Generic system for ground detection via avian3d contacts
//! - [`update_stepped_block_system`]: Generic system for detecting which block player stands on

pub mod avian_physics;
pub mod ground_detection;

// Re-export avian3d types for convenience
pub use avian3d::prelude::{
    AngularVelocity, Collider, Friction, GravityScale, LinearVelocity, LockedAxes, Position,
    Restitution, RigidBody, Rotation, SweptCcd,
};

// Re-export core physics types and functions from the avian integration
pub use avian_physics::{
    actions_to_camera_relative_input, apply_movement_step_to_components, compute_desired_velocity,
    compute_movement_step_from_actions, AvianSharedPhysicsPlugin, MovementInput, MovementMode,
    MovementStepResult, PlayerPhysicsBundle, AIR_FRICTION, GROUND_ACCELERATION, GROUND_FRICTION,
    PLAYER_CAPSULE_HEIGHT, PLAYER_CAPSULE_RADIUS, PLAYER_GRAVITY, PLAYER_JUMP_FORCE,
    PLAYER_QUERY_BOUNDS,
};

// Re-export ground detection from avian3d contacts
pub use ground_detection::{
    get_stepped_block, is_on_ground_from_contacts, update_ground_state_system,
    update_stepped_block_system, OnGround, SteppingOn, GROUND_NORMAL_THRESHOLD,
};
