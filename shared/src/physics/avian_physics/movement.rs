//! Movement and kinematics shared by client and server, with avian3d resolving collisions.

use avian3d::prelude::LinearVelocity;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;

use crate::{FLY_SPEED, FLY_VERTICAL_SPEED, WALK_SPEED};

/// Player physics constants
pub const PLAYER_GRAVITY: f32 = 50.0;
pub const PLAYER_JUMP_FORCE: f32 = 13.0;
/// Player capsule collider dimensions
pub const PLAYER_CAPSULE_RADIUS: f32 = 0.3;
pub const PLAYER_CAPSULE_HEIGHT: f32 = 1.1; // Total height = height + 2*radius = 1.7
/// Player query bounds for block lookups below the player (width, height, depth)
pub const PLAYER_QUERY_BOUNDS: Vec3 = Vec3::new(0.6, 1.7, 0.6);
/// Base acceleration rate for ground movement (units/s² per unit of friction)
pub const GROUND_ACCELERATION: f32 = 150.0;
/// Friction coefficient when on ground (higher = more responsive)
pub const GROUND_FRICTION: f32 = 8.0;
/// Friction coefficient when in air (lower = less air control)
pub const AIR_FRICTION: f32 = 2.0;

/// Represents the movement mode of an entity.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize,
)]
pub enum MovementMode {
    #[default]
    Walking,
    Flying,
}

impl MovementMode {
    pub fn speed(&self) -> f32 {
        match self {
            MovementMode::Walking => WALK_SPEED,
            MovementMode::Flying => FLY_SPEED,
        }
    }
}

/// Input state for a single physics tick.
#[derive(Debug, Clone, Default)]
pub struct MovementInput {
    /// Horizontal movement direction (normalized), relative to camera
    pub move_direction: Vec3,
    /// Whether the jump/fly-up input is pressed
    pub jump: bool,
    /// Whether the crouch/fly-down input is pressed
    pub crouch: bool,
    /// Camera forward direction (for movement orientation)
    pub camera_forward: Vec3,
    /// Camera right direction (for movement orientation)
    pub camera_right: Vec3,
}

/// Result of applying a single input frame to a player's physics state.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct PlayerStepOutput {
    /// The desired velocity (avian3d will apply this and resolve collisions)
    pub velocity: Vec3,
    /// Whether the player was on ground this frame (passed through from input state,
    /// used by callers for sound/visual effects - not modified by velocity computation)
    pub on_ground: bool,
    /// The current movement mode (may change if fly toggle was pressed)
    pub movement_mode: MovementMode,
}

/// Compute the desired velocity for a player based on input and current state.
///
/// This implements "custom kinematics" - we compute what velocity we want,
/// and avian3d handles collision resolution against chunk colliders.
pub fn compute_desired_velocity(
    velocity: Vec3,
    movement_mode: MovementMode,
    on_ground: bool,
    input: &MovementInput,
    delta_seconds: f32,
) -> Vec3 {
    let mut velocity = velocity;

    // Calculate world-space movement direction from input
    let forward_horizontal =
        Vec3::new(input.camera_forward.x, 0.0, input.camera_forward.z).normalize_or_zero();
    let right_horizontal =
        Vec3::new(input.camera_right.x, 0.0, input.camera_right.z).normalize_or_zero();

    // Transform local movement direction to world space
    let world_move_dir = if input.move_direction.length_squared() > 0.0 {
        let local_dir = input.move_direction.normalize();
        forward_horizontal * local_dir.z + right_horizontal * local_dir.x
    } else {
        Vec3::ZERO
    };

    // Calculate target horizontal velocity from input direction and mode speed
    let speed = movement_mode.speed();
    let target_velocity_xz = world_move_dir * speed;

    match movement_mode {
        MovementMode::Flying => {
            // Flying mode: direct velocity control
            velocity.x = target_velocity_xz.x;
            velocity.z = target_velocity_xz.z;
            velocity.y = (input.jump as i32 - input.crouch as i32) as f32 * FLY_VERTICAL_SPEED;

            velocity
        }
        MovementMode::Walking => {
            // Handle jumping (apply impulse if grounded)
            if input.jump && on_ground {
                velocity.y = PLAYER_JUMP_FORCE;
            }

            // Use ground or air friction based on contact state
            let friction = if on_ground {
                GROUND_FRICTION
            } else {
                AIR_FRICTION
            };

            // Smoothly accelerate horizontal velocity towards target
            let velocity_diff = Vec3::new(
                target_velocity_xz.x - velocity.x,
                0.0,
                target_velocity_xz.z - velocity.z,
            );

            let diff_magnitude = velocity_diff.length();
            if diff_magnitude > 0.0 {
                let accel_factor = (delta_seconds * friction * GROUND_ACCELERATION
                    / diff_magnitude.max(1.0))
                .min(1.0);
                velocity.x += velocity_diff.x * accel_factor;
                velocity.z += velocity_diff.z * accel_factor;
            }

            velocity
        }
    }
}

/// Convert transmittable actions to movement input
pub fn actions_to_movement_input(
    inputs: &HashSet<crate::messages::TransmittableAction>,
    camera_transform: &Transform,
) -> MovementInput {
    use crate::messages::TransmittableAction;

    let forward = camera_transform.forward().as_vec3();
    let right = camera_transform.right().as_vec3();

    let mut move_direction = Vec3::ZERO;
    let mut jump = false;
    let mut crouch = false;

    for action in inputs {
        match action {
            TransmittableAction::MoveForward => move_direction.z += 1.0,
            TransmittableAction::MoveBackward => move_direction.z -= 1.0,
            TransmittableAction::MoveRight => move_direction.x += 1.0,
            TransmittableAction::MoveLeft => move_direction.x -= 1.0,
            TransmittableAction::JumpOrFlyUp => jump = true,
            TransmittableAction::CrouchOrFlyDown => crouch = true,
            _ => {}
        }
    }

    MovementInput {
        move_direction,
        jump,
        crouch,
        camera_forward: forward,
        camera_right: right,
    }
}

/// Apply movement actions (including fly toggle) and compute desired velocity.
pub fn apply_player_input_step(
    velocity: Vec3,
    mut movement_mode: MovementMode,
    on_ground: bool,
    actions: &HashSet<crate::messages::TransmittableAction>,
    camera: &Transform,
    delta_seconds: f32,
) -> PlayerStepOutput {
    let mut current_velocity = velocity;

    // Handle fly-mode toggle
    if actions.contains(&crate::messages::TransmittableAction::ToggleFlyMode) {
        movement_mode = match movement_mode {
            MovementMode::Walking => MovementMode::Flying,
            MovementMode::Flying => MovementMode::Walking,
        };

        // Reset vertical velocity when returning to walking to avoid ghost motion
        if movement_mode == MovementMode::Walking {
            current_velocity = Vec3::ZERO;
        }
    }

    let movement_input = actions_to_movement_input(actions, camera);

    let new_velocity = compute_desired_velocity(
        current_velocity,
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
pub fn apply_player_input_to_components(
    linear_velocity: &mut LinearVelocity,
    movement_mode: &mut MovementMode,
    on_ground: bool,
    actions: &HashSet<crate::messages::TransmittableAction>,
    camera: &Transform,
    delta_seconds: f32,
) -> PlayerStepOutput {
    let step = apply_player_input_step(
        linear_velocity.0,
        *movement_mode,
        on_ground,
        actions,
        camera,
        delta_seconds,
    );

    // Write outputs back to components for simulation
    linear_velocity.0 = step.velocity;
    if step.movement_mode != *movement_mode {
        *movement_mode = step.movement_mode;
    }

    step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_velocity_computation_walking() {
        let velocity = Vec3::ZERO;
        let mode = MovementMode::Walking;
        let input = MovementInput {
            move_direction: Vec3::new(0.0, 0.0, 1.0), // Forward
            jump: false,
            crouch: false,
            camera_forward: Vec3::Z,
            camera_right: Vec3::X,
        };

        let new_velocity = compute_desired_velocity(velocity, mode, true, &input, 0.1);

        // Should have positive Z velocity (forward movement)
        assert!(new_velocity.z > 0.0);
    }

    #[test]
    fn test_velocity_computation_jump() {
        let velocity = Vec3::ZERO;
        let mode = MovementMode::Walking;
        let input = MovementInput {
            move_direction: Vec3::ZERO,
            jump: true,
            crouch: false,
            camera_forward: Vec3::Z,
            camera_right: Vec3::X,
        };

        let new_velocity = compute_desired_velocity(velocity, mode, true, &input, 0.1);

        // Should have upward velocity from jump
        assert_eq!(new_velocity.y, PLAYER_JUMP_FORCE);
    }

    #[test]
    fn test_flying_mode_direct_control() {
        let velocity = Vec3::ZERO;
        let mode = MovementMode::Flying;
        let input = MovementInput {
            move_direction: Vec3::ZERO,
            jump: true, // Fly up
            crouch: false,
            camera_forward: Vec3::Z,
            camera_right: Vec3::X,
        };

        let new_velocity = compute_desired_velocity(velocity, mode, false, &input, 0.1);

        // Should have upward velocity
        assert_eq!(new_velocity.y, FLY_VERTICAL_SPEED);
    }
}
