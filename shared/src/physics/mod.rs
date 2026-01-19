//! Shared physics simulation module using avian3d.
//!
//! This module provides physics types and velocity computation that can be used
//! by both client and server. The architecture follows the migration guidance:
//!
//! # Entity Types
//!
//! - **Players** (`DynamicPlayerPhysicsBundle`): `RigidBody::Dynamic` entities with capsule
//!   colliders. Avian3d handles collision resolution; we compute desired velocity.
//! - **Chunks** (`StaticChunkColliderBundle` via `meshing::ChunkColliderPlugin`):
//!   `RigidBody::Static` entities with trimesh colliders generated from greedy-meshed surfaces.
//!
//! # Movement Philosophy: Custom Kinematics + Physics Engine Collision
//!
//! We use a hybrid approach:
//! - **Velocity computation**: Custom code computes desired velocity from inputs
//! - **Collision resolution**: Avian3d integrates velocity and resolves collisions
//!
//! This gives us responsive, game-feel-tunable movement while leveraging avian3d's
//! robust collision detection against chunk geometry.
//!
//! # Server-Authoritative Networking Model
//!
//! The server is the single source of truth (SSOT) for player positions:
//!
//! 1. **Client** captures inputs and predicts movement locally for responsiveness
//! 2. **Client** sends inputs to server (with predicted position for diagnostics)
//! 3. **Server** processes inputs using identical velocity computation
//! 4. **Server** broadcasts authoritative position updates
//! 5. **Client** reconciles prediction with server state (see `ServerAuthorityReconciliationPlugin`)
//!
//! Both client and server use `apply_player_input_to_physics()` ensuring identical
//! velocity calculations (though collision resolution may differ due to timing).
//!
//! # Key Types and Functions
//!
//! - [`DynamicPlayerPhysicsBundle`]: Bundle for spawning player entities with all physics components
//! - [`SharedPhysicsWorldPlugin`]: Plugin that configures avian3d (gravity, collision detection)
//! - [`apply_player_input_to_physics`]: Applies input to ECS components (client & server)
//! - [`compute_player_desired_velocity`]: Pure function for velocity calculation
//! - [`sync_grounded_state`]: Generic system for ground detection via avian3d contacts
//! - [`sync_block_beneath_feet`]: Generic system for detecting which block player stands on

pub mod avian_physics;
pub mod ground_detection;

// Re-export avian3d types for convenience
pub use avian3d::prelude::{
    AngularVelocity, Collider, CollisionLayers, Friction, GravityScale, LinearVelocity, LockedAxes,
    Position, Restitution, RigidBody, Rotation, SweptCcd,
};

use crate::world::realm::Realm;

/// Build collision layers that isolate physics by realm.
/// Each realm gets its own bit; colliders only filter to the same realm bit.
pub fn collision_layers_for_realm(realm: Realm) -> CollisionLayers {
    // Bit 0 is Avian's default layer; shift by 1 to keep realm bits separate.
    let realm_bit = 1u32 << (realm as u32 + 1);
    CollisionLayers::new(realm_bit, realm_bit)
}

// Re-export core physics types and functions from the avian integration
pub use avian_physics::{
    apply_player_input_to_physics, compute_player_desired_velocity,
    compute_velocity_from_player_actions, player_actions_to_movement_input,
    DynamicPlayerPhysicsBundle, MovementInput, MovementMode, MovementStepResult,
    SharedPhysicsWorldPlugin, AIR_FRICTION, GROUND_ACCELERATION, GROUND_FRICTION,
    PLAYER_CAPSULE_HEIGHT, PLAYER_CAPSULE_RADIUS, PLAYER_GRAVITY, PLAYER_JUMP_VELOCITY,
    PLAYER_QUERY_BOUNDS,
};

// Re-export ground detection from avian3d contacts
pub use ground_detection::{
    check_grounded_from_collisions, find_block_beneath_feet, sync_block_beneath_feet,
    sync_grounded_state, BlockBeneathFeet, Grounded, MIN_GROUND_NORMAL_Y,
};
