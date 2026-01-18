//! Player input step for physics simulation.
//!
//! This module provides the `apply_player_input_step` function which processes
//! player input and computes the desired velocity. This function is used by
//! both client (for prediction) and server (for authority) to ensure identical
//! movement behavior.
//!
//! Note: This computes velocity only. Avian3d applies the velocity and handles
//! collision resolution against chunk colliders.

use bevy::platform::collections::HashSet;
use bevy::prelude::*;

use crate::messages::TransmittableAction;
use crate::physics::{
    actions_to_movement_input, compute_desired_velocity, MovementMode, PhysicsState,
};

/// Result of applying a single input frame to a player's physics state.
#[derive(Debug, Clone, Copy)]
pub struct PlayerStepOutput {
    /// The desired velocity (avian3d will apply this and resolve collisions)
    pub velocity: Vec3,
    /// Whether the player was on ground (used for jump logic)
    pub on_ground: bool,
    /// The current movement mode
    pub movement_mode: MovementMode,
}

/// Apply movement actions (including fly toggle) and compute desired velocity.
///
/// This is shared between client prediction and server authority to keep
/// movement behavior identical. The returned velocity should be applied to
/// the player's `LinearVelocity` component; avian3d will then handle
/// collision resolution against chunk colliders.
pub fn apply_player_input_step(
    state: &PhysicsState,
    actions: &HashSet<TransmittableAction>,
    camera: &Transform,
    delta_seconds: f32,
) -> PlayerStepOutput {
    let mut movement_mode = state.movement_mode;
    let mut velocity = state.velocity;

    // Handle fly-mode toggle
    if actions.contains(&TransmittableAction::ToggleFlyMode) {
        movement_mode = match movement_mode {
            MovementMode::Walking => MovementMode::Flying,
            MovementMode::Flying => MovementMode::Walking,
        };

        // Reset vertical velocity when returning to walking to avoid ghost motion
        if movement_mode == MovementMode::Walking {
            velocity = Vec3::ZERO;
        }
    }

    let movement_input = actions_to_movement_input(actions, camera);

    let sim_state = PhysicsState::from_components(
        state.position,
        velocity,
        movement_mode,
        state.realm,
        state.on_ground,
    );

    let result = compute_desired_velocity(&sim_state, &movement_input, delta_seconds);

    PlayerStepOutput {
        velocity: result.new_velocity,
        on_ground: result.on_ground,
        movement_mode,
    }
}
