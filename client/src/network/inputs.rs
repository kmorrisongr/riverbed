use bevy::prelude::*;
use bevy_renet::renet::RenetClient;
use shared::messages::ClientToServerMessage;
use shared::physics::LinearVelocity;

use crate::agents::PlayerControlled;
use crate::network::TargetServerState;
use crate::render::FpsCam;
use crate::ui::SelectedHotbarSlot;
use shared::net::lightyear_inputs::{action_mask_from_leafwing, PlayerInputAction};

use super::buffered_client::{CurrentFrameInputs, CurrentFrameInputsExt, SyncTime, SyncTimeExt};
use super::SendGameMessageExtension;
use shared::net::input_history::InputHistory;
use leafwing_input_manager::prelude::ActionState;

pub fn pre_input_update_system(
    mut frame_inputs: ResMut<CurrentFrameInputs>,
    mut input_history: ResMut<InputHistory>,
    mut sync_time: ResMut<SyncTime>,
) {
    sync_time.advance();

    let inputs_of_last_frame = frame_inputs.0.clone();
    input_history.push_frame(inputs_of_last_frame);

    frame_inputs.reset(sync_time.now_synced() as u64, sync_time.delta());
}

pub fn capture_player_inputs_system(
    player_actions: Query<&ActionState<PlayerInputAction>, With<PlayerControlled>>,
    mut frame_inputs: ResMut<CurrentFrameInputs>,
) {
    if frame_inputs.0.delta_ms == 0 {
        return;
    }

    let Ok(action_state) = player_actions.single() else {
        return;
    };

    frame_inputs.0.inputs = action_mask_from_leafwing(action_state);
}

pub fn update_frame_inputs_system(
    camera: Query<&Transform, With<FpsCam>>,
    player: Query<(&Transform, &LinearVelocity), (With<PlayerControlled>, Without<FpsCam>)>,
    selected_slot: Res<SelectedHotbarSlot>,
    mut frame_inputs: ResMut<CurrentFrameInputs>,
) {
    if frame_inputs.0.delta_ms == 0 {
        return;
    }

    if let Ok(camera_transform) = camera.single() {
        frame_inputs.0.camera = *camera_transform;
    }

    if let Ok((player_transform, linear_velocity)) = player.single() {
        frame_inputs.0.predicted_position = player_transform.translation;
        frame_inputs.0.predicted_velocity = linear_velocity.0;
    }

    frame_inputs.0.hotbar_slot = selected_slot.0 as u32;
}

pub fn upload_player_inputs_system(
    mut client: ResMut<RenetClient>,
    mut input_history: ResMut<InputHistory>,
    target: Res<crate::network::TargetServer>,
) {
    if client.is_disconnected() {
        input_history.clear_all();
        return;
    }

    if target.state != TargetServerState::FullyReady {
        return;
    }

    let frames = input_history.take_pending();
    if !frames.is_empty() {
        client.send_game_message(ClientToServerMessage::PlayerInputs(frames));
    }
}
