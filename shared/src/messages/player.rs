use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::items::{item_slots::inventory_serde, Stack};
use crate::physics::MovementMode;

use super::PlayerId;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Eq, Hash)]
#[repr(u8)]
pub enum TransmittableAction {
    MoveForward,
    MoveBackward,
    MoveLeft,
    MoveRight,
    JumpOrFlyUp,
    CrouchOrFlyDown,
    ToggleFlyMode,
    Hit,
    Modify,
}

/// Bitflag representation of player actions for compact storage and fast checks.
#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, Clone, Copy)]
pub struct ActionMask(pub u16);

impl ActionMask {
    #[inline]
    pub fn insert(&mut self, action: TransmittableAction) {
        self.0 |= 1 << (action as u16);
    }

    #[inline]
    pub fn contains(&self, action: TransmittableAction) -> bool {
        (self.0 & (1 << (action as u16))) != 0
    }
}

#[derive(Serialize, Deserialize, Default, PartialEq, Debug, Clone)]
pub struct PlayerSave {
    pub position: Vec3,
    pub camera_transform: Transform,
    pub is_flying: bool,
}

#[derive(Message, Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct ServerToClientPlayerSpawn {
    pub id: PlayerId,
    pub name: String,
    pub data: PlayerSave,
}

#[derive(Message, Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct ServerToClientPlayerUpdate {
    pub id: PlayerId,
    pub position: Vec3,
    pub velocity: Vec3,
    pub orientation: Quat,
    pub movement_mode: MovementMode,
    pub last_ack_time: u64,
    #[serde(with = "inventory_serde")]
    pub inventory: Box<[Stack]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ClientToServerPlayerInput {
    pub time_ms: u64,
    pub delta_ms: u64,
    pub inputs: ActionMask,
    pub camera: Transform,
    pub hotbar_slot: u32,
    pub predicted_position: Vec3,
    pub predicted_velocity: Vec3,
}
