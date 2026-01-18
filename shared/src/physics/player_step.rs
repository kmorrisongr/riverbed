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
    actions_to_movement_input, compute_desired_velocity, LinearVelocity, MovementMode, PhysicsState,
};
use crate::world::realm::Realm;

/// Result of applying a single input frame to a player's physics state.
#[derive(Debug, Clone, Copy)]
pub struct PlayerStepOutput {
    /// The desired velocity (avian3d will apply this and resolve collisions)
    pub velocity: Vec3,
    /// Whether the player was on ground this frame (passed through from input state,
    /// used by callers for sound/visual effects - not modified by velocity computation)
    pub on_ground: bool,
    /// The current movement mode (may change if fly toggle was pressed)
    pub movement_mode: MovementMode,
}

/// Apply movement actions (including fly toggle) and compute desired velocity.
///
/// This is shared between client prediction and server authority to keep
/// movement behavior identical. The returned velocity should be applied to
/// the player's `LinearVelocity` component; avian3d will then handle
/// collision resolution against chunk colliders.
pub fn apply_player_input_step(
    velocity: Vec3,
    mut movement_mode: MovementMode,
    on_ground: bool,
    actions: &HashSet<TransmittableAction>,
    camera: &Transform,
    delta_seconds: f32,
) -> PlayerStepOutput {
    let mut velocity = velocity;

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

    let new_velocity = compute_desired_velocity(
        velocity,
        movement_mode,
        on_ground,
        &movement_input,
        delta_seconds,
    );

    PlayerStepOutput {
        velocity: new_velocity,
        on_ground,
        movement_mode,
    }
}

/// Shared helper that applies a single input frame directly to ECS components.
///
/// This is used by both client prediction and the authoritative server to avoid
/// duplicating the "build inputs -> step -> write components" boilerplate.
pub fn apply_player_input_to_components(
    _transform: &Transform, // Keep for API compatibility if needed, though unused now
    linear_velocity: &mut LinearVelocity,
    movement_mode: &mut MovementMode,
    _realm: Realm, // Unused in velocity computation
    on_ground: bool,
    actions: &HashSet<TransmittableAction>,
    camera: &Transform,
    delta_seconds: f32,
) -> PlayerStepOutput {
    let step = apply_player_input_step(
        Vec3::from(linear_velocity.0),
        *movement_mode,
        on_ground,
        actions,
        camera,
        delta_seconds,
    );

    // Write outputs back to components for simulation
    linear_velocity.0 = step.velocity.into();
    if step.movement_mode != *movement_mode {
        *movement_mode = step.movement_mode;
    }

    step
}
