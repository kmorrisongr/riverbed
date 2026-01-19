use avian3d::prelude::*;
use bevy::prelude::*;
use lightyear::avian3d::plugin::AvianReplicationMode;
use lightyear::input::prelude::InputConfig;
use lightyear::prelude::input::leafwing;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use crate::net::lightyear_inputs::PlayerInputAction;
use crate::physics::{LinearVelocity, Position, Rotation, PLAYER_GRAVITY};
use crate::physics::MovementMode;

// --- Marker Components ------------------------------------------------------

/// Marker component for player character entities (replicated).
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct CharacterMarker;

/// Color associated with a player (replicated for rendering).
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerColor(pub Color);

/// Minimal protocol registration for lightyear-backed replication/prediction.
/// This mirrors the official avian3d example but uses our existing physics
/// components so we can begin wiring lightyear without disturbing the current
/// renet path.
pub struct LightyearProtocolPlugin;

impl Plugin for LightyearProtocolPlugin {
    fn build(&self, app: &mut App) {
        // Register leafwing input plugin for PlayerInputAction.
        // rebroadcast_inputs allows the server to relay inputs to other clients
        // for remote player prediction.
        app.add_plugins(leafwing::InputPlugin::<PlayerInputAction> {
            config: InputConfig::<PlayerInputAction> {
                rebroadcast_inputs: true,
                ..default()
            },
        });

        // Register marker components for replication.
        app.register_component::<CharacterMarker>();
        app.register_component::<PlayerColor>();
        app.register_component::<Name>();

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

/// Physics plugin setup for lightyear integration.
/// This configures avian3d physics to work with lightyear's rollback system.
/// Add this plugin instead of SharedPhysicsWorldPlugin when using lightyear.
pub struct LightyearPhysicsPlugin;

impl Plugin for LightyearPhysicsPlugin {
    fn build(&self, app: &mut App) {
        // Lightyear's avian plugin handles Position<->Transform sync during rollback.
        app.add_plugins(lightyear::avian3d::plugin::LightyearAvianPlugin {
            replication_mode: AvianReplicationMode::Position,
            ..default()
        });

        // Configure avian3d physics, disabling plugins that conflict with lightyear.
        app.add_plugins(
            PhysicsPlugins::default()
                .with_length_unit(1.0)
                .build()
                // Disable position<>transform sync - handled by lightyear_avian.
                .disable::<PhysicsTransformPlugin>()
                // Disable interpolation - handled by lightyear.
                .disable::<PhysicsInterpolationPlugin>(),
        );

        // Set gravity.
        app.insert_resource(Gravity(Vec3::new(0.0, -PLAYER_GRAVITY, 0.0)));
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
