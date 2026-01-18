//! Shared physics simulation module using avian3d.
//!
//! This module provides physics simulation code that can be used by both
//! client and server for authoritative movement. The server uses this
//! for authoritative position calculations, and the client uses it for
//! client-side prediction.
//!
//! The physics system integrates avian3d for physics types and character
//! controller functionality while maintaining custom voxel-based collision
//! detection via the BlockAccess trait.

pub mod avian_physics;
pub mod player_step;

// Re-export avian3d types for convenience
pub use avian3d::prelude::{
    Collider, GravityScale, LinearVelocity, LockedAxes, Position, RigidBody, Rotation,
};

// Re-export core physics types and functions from the avian integration
pub use avian_physics::{
    actions_to_movement_input, check_on_ground, get_stepped_block, simulate_physics_step,
    MovementInput, MovementMode, PhysicsState, PhysicsStepResult, PlayerPhysicsBundle,
    SharedPhysicsPlugin, ACC_MULT, PLAYER_AABB, PLAYER_GRAVITY, PLAYER_HALF_EXTENTS,
    PLAYER_JUMP_FORCE,
};

