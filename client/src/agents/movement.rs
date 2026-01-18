//! Client-side movement using avian3d physics.
//!
//! This module provides client-side movement prediction using custom kinematics
//! with avian3d handling collision resolution. We compute desired velocity,
//! set it on the player's LinearVelocity, and avian3d resolves collisions
//! against chunk trimesh colliders.

use bevy::prelude::*;
use shared::physics::{
    get_stepped_block, player_step::apply_player_input_step, sync_movement_mode_components,
    Flying, LinearVelocity, MovementMode, PhysicsState, PLAYER_QUERY_BOUNDS,
};
use shared::world::realm::Realm;

use crate::network::buffered_client::CurrentFrameInputs;
use crate::render::FpsCam;
use crate::world::ClientWorldMap;
use crate::Block;

use super::PlayerControlled;

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, update_stepped_block)
            .add_systems(Update, apply_movement_input);
    }
}

/// Block the entity is standing on (used for footstep sounds and friction info)
#[derive(Component)]
pub struct SteppingOn(pub Block);

#[derive(Component)]
pub struct Crouching(pub bool);

// Re-export shared movement mode marker (Flying already imported above)
pub use shared::physics::Walking;

/// Tracks the on_ground state for physics calculations
#[derive(Component, Default)]
pub struct OnGround(pub bool);

/// Updates the SteppingOn component to track what block the player is standing on.
/// This is used for footstep sounds and other effects.
fn update_stepped_block(
    world: Res<ClientWorldMap>,
    mut query: Query<(&Transform, &Realm, &mut SteppingOn)>,
) {
    for (transform, realm, mut stepping_on) in query.iter_mut() {
        stepping_on.0 = get_stepped_block(&*world, transform.translation, *realm, PLAYER_QUERY_BOUNDS);
    }
}

/// Applies movement input to compute desired velocity.
///
/// This system uses the same `apply_player_input_step` function that the server uses,
/// ensuring that client-side prediction produces identical velocity calculations.
/// Avian3d will then integrate the velocity and resolve collisions.
fn apply_movement_input(
    mut commands: Commands,
    time: Res<Time>,
    frame_inputs: Res<CurrentFrameInputs>,
    camera_query: Query<&Transform, With<FpsCam>>,
    mut player_query: Query<
        (
            Entity,
            &Transform,
            &mut LinearVelocity,
            &Realm,
            Option<&Flying>,
            Option<&OnGround>,
        ),
        (With<PlayerControlled>, Without<FpsCam>),
    >,
) {
    let Ok((entity, transform, mut linear_velocity, realm, free_fly_opt, on_ground_opt)) =
        player_query.single_mut()
    else {
        return;
    };

    // Skip if no delta time (first frame)
    if frame_inputs.0.delta_ms == 0 {
        return;
    }

    // Get camera transform for movement orientation
    let camera_transform = camera_query.single().copied().unwrap_or_default();

    // Build current physics state
    let was_flying = free_fly_opt.is_some();
    let current_mode = if was_flying {
        MovementMode::Flying
    } else {
        MovementMode::Walking
    };

    let on_ground = on_ground_opt.map(|og| og.0).unwrap_or(false);

    let state = PhysicsState::from_components(
        transform.translation,
        Vec3::from(linear_velocity.0),
        current_mode,
        *realm,
        on_ground,
    );

    // Compute desired velocity
    let delta_seconds = time.delta_secs();
    let step = apply_player_input_step(
        &state,
        &frame_inputs.0.inputs,
        &camera_transform,
        delta_seconds,
    );

    // Set velocity - avian3d will integrate and resolve collisions
    linear_velocity.0 = step.velocity.into();

    // Update on_ground state
    if on_ground_opt.is_none() {
        commands.entity(entity).insert(OnGround(step.on_ground));
    }

    // Sync movement mode ECS components if changed
    sync_movement_mode_components(&mut commands, entity, step.movement_mode, was_flying);
}
