//! Client-side movement systems.
//!
//! With lightyear, movement prediction is handled by the handle_character_actions
//! system in lightyear_client.rs. This module provides ground state synchronization
//! which is still needed for proper physics simulation.

use bevy::prelude::*;
use shared::physics::{sync_block_beneath_feet, sync_grounded_state};

use crate::world::ClientWorldMap;

use super::PlayerControlled;

pub struct ClientSideMovementPredictionPlugin;

impl Plugin for ClientSideMovementPredictionPlugin {
    fn build(&self, app: &mut App) {
        // Use shared systems for ground state updates, parameterized by PlayerControlled marker.
        // Movement physics is handled by lightyear's handle_character_actions system.
        app.add_systems(
            PreUpdate,
            sync_block_beneath_feet::<PlayerControlled, ClientWorldMap>,
        )
        .add_systems(Update, sync_grounded_state::<PlayerControlled>);
    }
}

/// Whether the player is currently crouching.
#[derive(Component)]
pub struct Crouching(pub bool);

pub use shared::physics::BlockBeneathFeet;
