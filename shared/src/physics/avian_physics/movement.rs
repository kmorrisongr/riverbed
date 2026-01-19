use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;

use crate::messages::{ActionMask, TransmittableAction};
use crate::{FLY_SPEED, FLY_VERTICAL_SPEED, WALK_SPEED};

pub const PLAYER_GRAVITY: f32 = 50.0;

pub const PLAYER_JUMP_VELOCITY: f32 = 13.0;

pub const PLAYER_CAPSULE_RADIUS: f32 = 0.3;

pub const PLAYER_CAPSULE_HEIGHT: f32 = 1.1;

pub const PLAYER_QUERY_BOUNDS: Vec3 = Vec3::new(0.6, 1.7, 0.6);

pub const GROUND_ACCELERATION: f32 = 150.0;

pub const GROUND_FRICTION: f32 = 8.0;

pub const AIR_FRICTION: f32 = 2.0;

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

#[derive(Debug, Clone, Default)]
pub struct MovementInput {
    pub input_axes: Vec3,
    pub jump: bool,
    pub crouch: bool,
    pub camera_forward: Vec3,
    pub camera_right: Vec3,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct MovementStepResult {
    pub velocity: Vec3,
    pub on_ground: bool,
    pub movement_mode: MovementMode,
}

pub fn compute_player_desired_velocity(
    velocity: Vec3,
    movement_mode: MovementMode,
    on_ground: bool,
    input: &MovementInput,
    delta_seconds: f32,
) -> Vec3 {
    let mut velocity = velocity;

    let forward_horizontal =
        Vec3::new(input.camera_forward.x, 0.0, input.camera_forward.z).normalize_or_zero();
    let right_horizontal =
        Vec3::new(input.camera_right.x, 0.0, input.camera_right.z).normalize_or_zero();

    let world_move_dir = if input.input_axes.length_squared() > 0.0 {
        let local_dir = input.input_axes.normalize();
        forward_horizontal * local_dir.z + right_horizontal * local_dir.x
    } else {
        Vec3::ZERO
    };

    let speed = movement_mode.speed();
    let target_velocity_xz = world_move_dir * speed;

    match movement_mode {
        MovementMode::Flying => {
            velocity.x = target_velocity_xz.x;
            velocity.z = target_velocity_xz.z;
            velocity.y = (input.jump as i32 - input.crouch as i32) as f32 * FLY_VERTICAL_SPEED;

            velocity
        }
        MovementMode::Walking => {
            if input.jump && on_ground {
                velocity.y = PLAYER_JUMP_VELOCITY;
            }

            let friction = if on_ground {
                GROUND_FRICTION
            } else {
                AIR_FRICTION
            };

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

    if actions.contains(TransmittableAction::ToggleFlyMode) {
        movement_mode = match movement_mode {
            MovementMode::Walking => MovementMode::Flying,
            MovementMode::Flying => MovementMode::Walking,
        };

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
            input_axes: Vec3::new(0.0, 0.0, 1.0),
            jump: false,
            crouch: false,
            camera_forward: Vec3::Z,
            camera_right: Vec3::X,
        };

        let new_velocity = compute_player_desired_velocity(velocity, mode, true, &input, 0.1);
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
        assert_eq!(new_velocity.y, PLAYER_JUMP_VELOCITY);
    }

    #[test]
    fn test_flying_mode_direct_control() {
        let velocity = Vec3::ZERO;
        let mode = MovementMode::Flying;
        let input = MovementInput {
            input_axes: Vec3::ZERO,
            jump: true,
            crouch: false,
            camera_forward: Vec3::Z,
            camera_right: Vec3::X,
        };

        let new_velocity = compute_player_desired_velocity(velocity, mode, false, &input, 0.1);
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
