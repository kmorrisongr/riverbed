use bevy::prelude::*;
use lightyear::prelude::*;

use crate::physics::{LinearVelocity, Position, Rotation};
use crate::physics::MovementMode;

/// Minimal protocol registration for lightyear-backed replication/prediction.
/// This mirrors the official avian3d example but uses our existing physics
/// components so we can begin wiring lightyear without disturbing the current
/// renet path.
pub struct LightyearProtocolPlugin;

impl Plugin for LightyearProtocolPlugin {
    fn build(&self, app: &mut App) {
        // Position/Rotation mirror the avian3d example: predicted with visual
        // interpolation and mild rollback tolerance.
        app.register_component::<Position>()
            .add_prediction()
            .add_should_rollback(position_should_rollback)
            .add_linear_correction_fn()
            .add_linear_interpolation();

        app.register_component::<Rotation>()
            .add_prediction()
            .add_should_rollback(rotation_should_rollback)
            .add_linear_correction_fn()
            .add_linear_interpolation();

        // Velocity is predicted/rollback-aware but not interpolated for visuals.
        app.register_component::<LinearVelocity>()
            .add_prediction()
            .add_should_rollback(linear_velocity_should_rollback);

        // Movement mode flips infrequently; treat any change as a rollback event.
        app.register_component::<MovementMode>()
            .add_prediction()
            .add_should_rollback(movement_mode_should_rollback);
    }
}

// --- Rollback thresholds ----------------------------------------------------

const POSITION_EPSILON: f32 = 0.01;
const ROTATION_EPSILON: f32 = 0.01;
const VELOCITY_EPSILON: f32 = 0.01;

fn position_should_rollback(this: &Position, that: &Position) -> bool {
    (this.0 - that.0).length() >= POSITION_EPSILON
}

fn rotation_should_rollback(this: &Rotation, that: &Rotation) -> bool {
    this.angle_between(*that) >= ROTATION_EPSILON
}

fn linear_velocity_should_rollback(this: &LinearVelocity, that: &LinearVelocity) -> bool {
    (this.0 - that.0).length() >= VELOCITY_EPSILON
}

fn movement_mode_should_rollback(this: &MovementMode, that: &MovementMode) -> bool {
    this != that
}
