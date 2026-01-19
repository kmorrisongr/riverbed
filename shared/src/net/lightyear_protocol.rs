use avian3d::prelude::*;
use bevy::prelude::*;
use lightyear::avian3d::plugin::AvianReplicationMode;
use lightyear::input::prelude::InputConfig;
use lightyear::prelude::input::leafwing;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use crate::block::Block;
use crate::items::item_slots::ItemHolder;
use crate::net::lightyear_inputs::PlayerInputAction;
use crate::physics::MovementMode;
use crate::physics::{LinearVelocity, Position, Rotation, PLAYER_GRAVITY};
use crate::world::pos::pos3d::BlockPos;

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct CharacterMarker;

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerColor(pub Color);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct CameraOrientation {
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SelectedHotbarSlot(pub u8);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BlockInteractionRequest {
    pub position: BlockPos,
    pub new_block: Block,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BlockChangeConfirm {
    pub position: BlockPos,
    pub old_block: Block,
    pub new_block: Block,
}

pub struct BlockInteractionChannel;

impl CameraOrientation {
    pub fn new(yaw: f32, pitch: f32) -> Self {
        Self { yaw, pitch }
    }

    pub fn to_transform(&self) -> Transform {
        Transform::from_rotation(
            Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(self.pitch),
        )
    }
}

pub struct LightyearProtocolPlugin;

impl Plugin for LightyearProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(leafwing::InputPlugin::<PlayerInputAction> {
            config: InputConfig::<PlayerInputAction> {
                rebroadcast_inputs: true,
                ..default()
            },
        });

        app.register_component::<CharacterMarker>();
        app.register_component::<PlayerColor>();
        app.register_component::<Name>();

        app.register_component::<CameraOrientation>()
            .add_prediction();

        app.register_component::<SelectedHotbarSlot>()
            .add_prediction();

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

        app.register_component::<MovementMode>()
            .add_prediction()
            .add_should_rollback(movement_mode_should_rollback);

        app.register_component::<ItemHolder>();

        app.add_channel::<BlockInteractionChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);

        app.register_message::<BlockInteractionRequest>()
            .add_direction(NetworkDirection::ClientToServer);

        app.register_message::<BlockChangeConfirm>()
            .add_direction(NetworkDirection::ServerToClient);
    }
}

pub struct LightyearPhysicsPlugin;

impl Plugin for LightyearPhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(lightyear::avian3d::plugin::LightyearAvianPlugin {
            replication_mode: AvianReplicationMode::Position,
            ..default()
        });

        app.add_plugins(
            PhysicsPlugins::default()
                .with_length_unit(1.0)
                .build()
                .disable::<PhysicsTransformPlugin>()
                .disable::<PhysicsInterpolationPlugin>(),
        );

        app.insert_resource(Gravity(Vec3::new(0.0, -PLAYER_GRAVITY, 0.0)));
    }
}

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
