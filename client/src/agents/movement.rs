use bevy::prelude::*;
use shared::physics::{
    apply_player_input_to_physics, sync_block_beneath_feet, sync_grounded_state, Grounded,
    LinearVelocity, MovementMode,
};

use crate::network::buffered_client::CurrentFrameInputs;
use crate::render::FpsCam;
use crate::world::ClientWorldMap;

use super::PlayerControlled;

pub struct ClientSideMovementPredictionPlugin;

impl Plugin for ClientSideMovementPredictionPlugin {
    fn build(&self, app: &mut App) {
        // Use shared systems for ground state updates, parameterized by PlayerControlled marker
        app.add_systems(
            PreUpdate,
            sync_block_beneath_feet::<PlayerControlled, ClientWorldMap>,
        )
        .add_systems(
            Update,
            (
                sync_grounded_state::<PlayerControlled>,
                apply_predicted_movement_input,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
pub struct Crouching(pub bool);

pub use shared::physics::BlockBeneathFeet;

fn apply_predicted_movement_input(
    time: Res<Time>,
    frame_inputs: Res<CurrentFrameInputs>,
    camera_query: Query<&Transform, With<FpsCam>>,
    mut player_query: Query<
        (&mut LinearVelocity, &mut MovementMode, &Grounded),
        (With<PlayerControlled>, Without<FpsCam>),
    >,
) {
    let Ok((mut linear_velocity, mut movement_mode, grounded)) = player_query.single_mut() else {
        return;
    };

    // Skip if no delta time (first frame)
    if frame_inputs.0.delta_ms == 0 {
        return;
    }

    // Get camera transform for movement orientation
    let camera_transform = camera_query.single().copied().unwrap_or_default();

    let delta_seconds = time.delta_secs();
    apply_player_input_to_physics(
        &mut linear_velocity,
        &mut movement_mode,
        grounded.0,
        &frame_inputs.0.inputs,
        &camera_transform,
        delta_seconds,
    );
}
