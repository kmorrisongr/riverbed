//! Client-side movement using avian3d physics.
//!
//! This module provides client-side movement prediction using custom kinematics
//! with avian3d handling collision resolution. We compute desired velocity,
//! set it on the player's LinearVelocity, and avian3d resolves collisions
//! against chunk trimesh colliders.

use bevy::prelude::*;
use shared::physics::{
    apply_player_input_to_components, update_ground_state_system, update_stepped_block_system,
    LinearVelocity, MovementMode, OnGround,
};

use crate::network::buffered_client::CurrentFrameInputs;
use crate::render::FpsCam;
use crate::world::ClientWorldMap;

use super::PlayerControlled;

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        // Use shared systems for ground state updates, parameterized by PlayerControlled marker
        app.add_systems(
            PreUpdate,
            update_stepped_block_system::<PlayerControlled, ClientWorldMap>,
        )
        .add_systems(
            Update,
            (
                update_ground_state_system::<PlayerControlled>,
                apply_movement_input,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
pub struct Crouching(pub bool);

// Re-export shared physics components for other client modules
pub use shared::physics::SteppingOn;

/// Applies movement input to compute desired velocity.
///
/// This system uses the same `apply_player_input_step` function that the server uses,
/// ensuring that client-side prediction produces identical velocity calculations.
/// Avian3d will then integrate the velocity and resolve collisions.
fn apply_movement_input(
    time: Res<Time>,
    frame_inputs: Res<CurrentFrameInputs>,
    camera_query: Query<&Transform, With<FpsCam>>,
    mut player_query: Query<
        (&mut LinearVelocity, &mut MovementMode, &OnGround),
        (With<PlayerControlled>, Without<FpsCam>),
    >,
) {
    let Ok((mut linear_velocity, mut movement_mode, on_ground)) = player_query.single_mut() else {
        return;
    };

    // Skip if no delta time (first frame)
    if frame_inputs.0.delta_ms == 0 {
        return;
    }

    // Get camera transform for movement orientation
    let camera_transform = camera_query.single().copied().unwrap_or_default();

    let delta_seconds = time.delta_secs();
    apply_player_input_to_components(
        &mut linear_velocity,
        &mut movement_mode,
        on_ground.0,
        &frame_inputs.0.inputs,
        &camera_transform,
        delta_seconds,
    );
}
