use avian3d::prelude::*;
use bevy::prelude::*;

use super::{MovementMode, PLAYER_CAPSULE_HEIGHT, PLAYER_CAPSULE_RADIUS};
use crate::physics::collision_layers_for_realm;
use crate::physics::ground_detection::{BlockBeneathFeet, Grounded};
use crate::world::realm::Realm;

#[derive(Bundle)]
pub struct DynamicPlayerPhysicsBundle {
    pub rigid_body: RigidBody,
    pub collider: Collider,
    pub linear_velocity: LinearVelocity,
    pub angular_velocity: AngularVelocity,
    pub locked_axes: LockedAxes,
    pub gravity_scale: GravityScale,
    pub friction: Friction,
    pub restitution: Restitution,
    pub ccd: SweptCcd,
    pub position: Position,
    pub rotation: Rotation,
    pub collision_layers: CollisionLayers,
    pub movement_mode: MovementMode,
    pub grounded: Grounded,
    pub block_beneath_feet: BlockBeneathFeet,
}

impl DynamicPlayerPhysicsBundle {
    pub fn new_in_realm(realm: Realm) -> Self {
        Self {
            rigid_body: RigidBody::Dynamic,
            collider: Collider::capsule(PLAYER_CAPSULE_RADIUS, PLAYER_CAPSULE_HEIGHT),
            linear_velocity: LinearVelocity::default(),
            angular_velocity: AngularVelocity::default(),
            locked_axes: LockedAxes::ROTATION_LOCKED,
            gravity_scale: GravityScale(1.0),
            friction: Friction::new(0.0),
            restitution: Restitution::new(0.0),
            ccd: SweptCcd::default(),
            position: Position::default(),
            rotation: Rotation::default(),
            collision_layers: collision_layers_for_realm(realm),
            movement_mode: MovementMode::default(),
            grounded: Grounded::default(),
            block_beneath_feet: BlockBeneathFeet::default(),
        }
    }

    pub fn from_transform(transform: &Transform, realm: Realm) -> Self {
        let mut bundle = Self::new_in_realm(realm);
        bundle.position = Position(transform.translation);
        bundle.rotation = Rotation(transform.rotation);
        bundle
    }
}

impl Default for DynamicPlayerPhysicsBundle {
    fn default() -> Self {
        Self::new_in_realm(Realm::Overworld)
    }
}
