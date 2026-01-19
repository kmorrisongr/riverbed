//! Client-side movement using avian3d physics.
//!
//! This module provides client-side movement prediction using custom kinematics
//! with avian3d handling collision resolution. We compute desired velocity,
//! set it on the player's LinearVelocity, and avian3d resolves collisions
//! against chunk trimesh colliders.

use bevy::prelude::*;
use shared::physics::{
    apply_player_input_to_physics, sync_block_beneath_feet, sync_grounded_state, Grounded,
    LinearVelocity, MovementMode,
};

use crate::network::buffered_client::CurrentFrameInputs;
use crate::render::FpsCam;
use crate::world::ClientWorldMap;

use super::PlayerControlled;

/// Plugin that handles client-side movement prediction using avian3d physics.
///
/// This plugin implements the client half of the server-authoritative model:
/// - Captures player inputs and applies them locally for responsive gameplay
/// - Uses the same velocity computation as the server (via shared code)
/// - Avian3d handles collision resolution against chunk colliders
///
/// The server remains authoritative; see `network::reconciliation` for correction handling.
pub struct ClientSideMovementPredictionPlugin;

impl Plugin for ClientSideMovementPredictionPlugin {
    fn build(&self, app: &mut App) {
        // Use shared systems for ground state updates, parameterized by PlayerControlled marker
        app.add_systems(
            PreUpdate,
            sync_block_beneath_feet::<PlayerControlled, ClientWorldMap>,
        )
        .add_systems(
            Update,
            (
                sync_grounded_state::<PlayerControlled>,
                apply_predicted_movement_input,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
pub struct Crouching(pub bool);

// Re-export shared physics components for other client modules
pub use shared::physics::BlockBeneathFeet;

/// Applies movement input to compute predicted velocity for the local player.
///
/// This system uses the same `apply_player_input_to_physics` function that the server uses,
/// ensuring that client-side prediction produces identical velocity calculations.
/// Avian3d will then integrate the velocity and resolve collisions.
fn apply_predicted_movement_input(
    time: Res<Time>,
    frame_inputs: Res<CurrentFrameInputs>,
    camera_query: Query<&Transform, With<FpsCam>>,
    mut player_query: Query<
        (&mut LinearVelocity, &mut MovementMode, &Grounded),
        (With<PlayerControlled>, Without<FpsCam>),
    >,
) {
    let Ok((mut linear_velocity, mut movement_mode, grounded)) = player_query.single_mut() else {
        return;
    };

    // Skip if no delta time (first frame)
    if frame_inputs.0.delta_ms == 0 {
        return;
    }

    // Get camera transform for movement orientation
    let camera_transform = camera_query.single().copied().unwrap_or_default();

    let delta_seconds = time.delta_secs();
    apply_player_input_to_physics(
        &mut linear_velocity,
        &mut movement_mode,
        grounded.0,
        &frame_inputs.0.inputs,
        &camera_transform,
        delta_seconds,
    );
}
