//! Movement and kinematics shared by client and server, with avian3d resolving collisions.

use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;

use crate::messages::{ActionMask, TransmittableAction};
use crate::{FLY_SPEED, FLY_VERTICAL_SPEED, WALK_SPEED};

/// Gravitational acceleration for players (units/s²).
/// Higher values = faster falling.
pub const PLAYER_GRAVITY: f32 = 50.0;

/// Instantaneous vertical velocity applied when jumping (units/s).
pub const PLAYER_JUMP_VELOCITY: f32 = 13.0;

/// Radius of the player's capsule collider (meters).
pub const PLAYER_CAPSULE_RADIUS: f32 = 0.3;

/// Height of the cylindrical portion of the player's capsule collider.
/// Total player height = PLAYER_CAPSULE_HEIGHT + 2 * PLAYER_CAPSULE_RADIUS = 1.7m
pub const PLAYER_CAPSULE_HEIGHT: f32 = 1.1;

/// Axis-aligned bounding box for querying blocks near the player's feet.
/// Used for ground detection and stepped-block queries (footstep sounds, etc.).
/// Format: (width, height, depth) in meters.
pub const PLAYER_QUERY_BOUNDS: Vec3 = Vec3::new(0.6, 1.7, 0.6);

/// Base acceleration rate for ground movement (units/s² per unit of friction).
/// Combined with friction coefficient to determine how quickly velocity changes.
pub const GROUND_ACCELERATION: f32 = 150.0;

/// Friction coefficient when standing on ground.
/// Higher values = more responsive movement (faster acceleration/deceleration).
pub const GROUND_FRICTION: f32 = 8.0;

/// Friction coefficient when airborne.
/// Lower values = less air control (maintains momentum better).
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
    pub input_axes: Vec3,
    pub jump: bool,
    pub crouch: bool,
    pub camera_forward: Vec3,
    pub camera_right: Vec3,
}

/// Result of computing a single frame of player movement.
///
/// This struct captures the output of velocity computation for a single physics tick.
/// It's used by both client (for prediction) and server (for authoritative simulation)
/// to ensure identical movement logic.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct MovementStepResult {
    pub velocity: Vec3,
    pub on_ground: bool,
    pub movement_mode: MovementMode,
}

/// Compute the desired velocity for a player based on input and current state.
///
/// This implements "custom kinematics" - we compute what velocity we want,
/// and avian3d handles collision resolution against chunk colliders.
pub fn compute_player_desired_velocity(
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
    let world_move_dir = if input.input_axes.length_squared() > 0.0 {
        let local_dir = input.input_axes.normalize();
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
                velocity.y = PLAYER_JUMP_VELOCITY;
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

/// Convert player action flags to camera-relative movement input.
pub fn player_actions_to_movement_input(
    actions: &ActionMask,
    camera_transform: &Transform,
) -> MovementInput {
    let forward = camera_transform.forward().as_vec3();
    let right = camera_transform.right().as_vec3();

    let mut input_axes = Vec3::ZERO;
    let mut jump = false;
    let mut crouch = false;

    if actions.contains(TransmittableAction::MoveForward) {
        input_axes.z += 1.0;
    }
    if actions.contains(TransmittableAction::MoveBackward) {
        input_axes.z -= 1.0;
    }
    if actions.contains(TransmittableAction::MoveRight) {
        input_axes.x += 1.0;
    }
    if actions.contains(TransmittableAction::MoveLeft) {
        input_axes.x -= 1.0;
    }
    if actions.contains(TransmittableAction::JumpOrFlyUp) {
        jump = true;
    }
    if actions.contains(TransmittableAction::CrouchOrFlyDown) {
        crouch = true;
    }

    MovementInput {
        input_axes,
        jump,
        crouch,
        camera_forward: forward,
        camera_right: right,
    }
}

pub fn compute_velocity_from_player_actions(
    velocity: Vec3,
    mut movement_mode: MovementMode,
    on_ground: bool,
    actions: &ActionMask,
    camera: &Transform,
    delta_seconds: f32,
) -> MovementStepResult {
    let mut current_velocity = velocity;

    // Handle fly-mode toggle
    if actions.contains(TransmittableAction::ToggleFlyMode) {
        movement_mode = match movement_mode {
            MovementMode::Walking => MovementMode::Flying,
            MovementMode::Flying => MovementMode::Walking,
        };

        // Reset vertical velocity when returning to walking to avoid ghost motion
        if movement_mode == MovementMode::Walking {
            current_velocity = Vec3::ZERO;
        }
    }

    let movement_input = player_actions_to_movement_input(actions, camera);

    let new_velocity = compute_player_desired_velocity(
        current_velocity,
        movement_mode,
        on_ground,
        &movement_input,
        delta_seconds,
    );

    MovementStepResult {
        velocity: new_velocity,
        on_ground,
        movement_mode,
    }
}

pub fn apply_player_input_to_physics(
    linear_velocity: &mut LinearVelocity,
    movement_mode: &mut MovementMode,
    on_ground: bool,
    actions: &ActionMask,
    camera: &Transform,
    delta_seconds: f32,
) -> MovementStepResult {
    let step = compute_velocity_from_player_actions(
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
    use crate::messages::TransmittableAction;

    #[test]
    fn test_velocity_computation_walking() {
        let velocity = Vec3::ZERO;
        let mode = MovementMode::Walking;
        let input = MovementInput {
            input_axes: Vec3::new(0.0, 0.0, 1.0), // Forward
            jump: false,
            crouch: false,
            camera_forward: Vec3::Z,
            camera_right: Vec3::X,
        };

        let new_velocity = compute_player_desired_velocity(velocity, mode, true, &input, 0.1);

        // Should have positive Z velocity (forward movement)
        assert!(new_velocity.z > 0.0);
    }

    #[test]
    fn test_velocity_computation_jump() {
        let velocity = Vec3::ZERO;
        let mode = MovementMode::Walking;
        let input = MovementInput {
            input_axes: Vec3::ZERO,
            jump: true,
            crouch: false,
            camera_forward: Vec3::Z,
            camera_right: Vec3::X,
        };

        let new_velocity = compute_player_desired_velocity(velocity, mode, true, &input, 0.1);

        // Should have upward velocity from jump
        assert_eq!(new_velocity.y, PLAYER_JUMP_VELOCITY);
    }

    #[test]
    fn test_flying_mode_direct_control() {
        let velocity = Vec3::ZERO;
        let mode = MovementMode::Flying;
        let input = MovementInput {
            input_axes: Vec3::ZERO,
            jump: true, // Fly up
            crouch: false,
            camera_forward: Vec3::Z,
            camera_right: Vec3::X,
        };

        let new_velocity = compute_player_desired_velocity(velocity, mode, false, &input, 0.1);

        // Should have upward velocity
        assert_eq!(new_velocity.y, FLY_VERTICAL_SPEED);
    }

    #[test]
    fn test_action_mask_contains() {
        let mut mask = ActionMask::default();
        mask.insert(TransmittableAction::MoveForward);
        assert!(mask.contains(TransmittableAction::MoveForward));
        assert!(!mask.contains(TransmittableAction::MoveBackward));
    }
}
