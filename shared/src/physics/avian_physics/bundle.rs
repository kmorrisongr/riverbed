//! Player physics bundle definition for avian3d.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{MovementMode, PLAYER_CAPSULE_HEIGHT, PLAYER_CAPSULE_RADIUS};
use crate::physics::ground_detection::{OnGround, SteppingOn};

/// Physics bundle for dynamic player entities using avian3d.
///
/// This bundle contains all components needed for physics simulation:
/// - Avian3d components (RigidBody::Dynamic, Collider, velocities, etc.)
/// - Movement state tracking (MovementMode, OnGround, SteppingOn)
///
/// Both client and server use this bundle when spawning player entities,
/// ensuring consistent physics behavior across the network.
#[derive(Bundle)]
pub struct DynamicPlayerPhysicsBundle {
    // Avian3d physics components
    pub rigid_body: RigidBody,
    pub collider: Collider,
    pub linear_velocity: LinearVelocity,
    pub angular_velocity: AngularVelocity,
    pub locked_axes: LockedAxes,
    pub gravity_scale: GravityScale,
    pub friction: Friction,
    pub restitution: Restitution,
    pub ccd: SweptCcd,
    // Movement state components (shared with ground detection systems)
    pub movement_mode: MovementMode,
    pub on_ground: OnGround,
    pub stepping_on: SteppingOn,
}

impl DynamicPlayerPhysicsBundle {
    /// Create a new dynamic player physics bundle with a capsule collider.
    pub fn new() -> Self {
        Self {
            rigid_body: RigidBody::Dynamic,
            collider: Collider::capsule(PLAYER_CAPSULE_RADIUS, PLAYER_CAPSULE_HEIGHT),
            linear_velocity: LinearVelocity::default(),
            angular_velocity: AngularVelocity::default(),
            // Lock all rotation to prevent tipping over
            locked_axes: LockedAxes::ROTATION_LOCKED,
            // Use standard gravity (configured via Gravity resource)
            gravity_scale: GravityScale(1.0),
            // Low friction for responsive movement
            friction: Friction::new(0.1),
            // No bounce
            restitution: Restitution::new(0.0),
            // Enable continuous collision detection for fast movement
            ccd: SweptCcd::default(),
            // Movement state defaults
            movement_mode: MovementMode::default(),
            on_ground: OnGround::default(),
            stepping_on: SteppingOn::default(),
        }
    }
}

impl Default for DynamicPlayerPhysicsBundle {
    fn default() -> Self {
        Self::new()
    }
}
