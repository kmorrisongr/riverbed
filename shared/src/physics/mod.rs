pub mod avian_physics;
pub mod ground_detection;
pub use avian3d::prelude::{
    AngularVelocity, Collider, CollisionLayers, Friction, GravityScale, LinearVelocity, LockedAxes,
    Position, Restitution, RigidBody, Rotation, SweptCcd,
};

use crate::world::realm::Realm;

pub fn collision_layers_for_realm(realm: Realm) -> CollisionLayers {
    let realm_bit = 1u32 << (realm as u32 + 1);
    CollisionLayers::new(realm_bit, realm_bit)
}
pub use avian_physics::{
    apply_player_input_to_physics, compute_player_desired_velocity,
    compute_velocity_from_player_actions, player_actions_to_movement_input,
    DynamicPlayerPhysicsBundle, MovementInput, MovementMode, MovementStepResult,
    SharedPhysicsWorldPlugin, AIR_FRICTION, GROUND_ACCELERATION, GROUND_FRICTION,
    PLAYER_CAPSULE_HEIGHT, PLAYER_CAPSULE_RADIUS, PLAYER_GRAVITY, PLAYER_JUMP_VELOCITY,
    PLAYER_QUERY_BOUNDS,
};
pub use ground_detection::{
    check_grounded_from_collisions, find_block_beneath_feet, sync_block_beneath_feet,
    sync_grounded_state, BlockBeneathFeet, Grounded, MIN_GROUND_NORMAL_Y_THRESHOLD,
};
