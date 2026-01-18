//! Player physics bundle definition for avian3d.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{PLAYER_CAPSULE_HEIGHT, PLAYER_CAPSULE_RADIUS};

/// Avian3d player physics bundle with dynamic rigid body.
#[derive(Bundle)]
pub struct PlayerPhysicsBundle {
    pub rigid_body: RigidBody,
    pub collider: Collider,
    pub linear_velocity: LinearVelocity,
    pub angular_velocity: AngularVelocity,
    pub locked_axes: LockedAxes,
    pub gravity_scale: GravityScale,
    pub friction: Friction,
    pub restitution: Restitution,
    pub ccd: SweptCcd,
}

impl PlayerPhysicsBundle {
    /// Create a new player physics bundle using a capsule collider.
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
        }
    }
}

impl Default for PlayerPhysicsBundle {
    fn default() -> Self {
        Self::new()
    }
}
