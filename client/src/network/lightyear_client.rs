#![allow(dead_code)]
//! Minimal scaffolding to start threading player inputs toward a lightyear
//! client pipeline. This is feature-gated behind `lightyear-net` so it can
//! iterate without impacting the current Renet path.

use bevy::prelude::*;
use shared::messages::ActionMask;
use shared::net::lightyear_inputs::{action_mask_from_leafwing, PlayerInputAction};

use crate::agents::PlayerControlled;
use crate::render::FpsCam;
use crate::network::buffered_client::SyncTime;

/// Latest input we intend to feed into a Lightyear input timeline.
#[derive(Resource, Default, Debug, Clone)]
pub struct LightyearInputSnapshot {
    pub tick_ms: u64,
    pub action_mask: ActionMask,
    pub camera: Transform,
}

pub struct LightyearClientPlugin;

impl Plugin for LightyearClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LightyearInputSnapshot>()
            // Capture inputs once per frame; this can be swapped to Lightyear's input
            // timeline ingestion when the transport is wired.
            .add_systems(PreUpdate, capture_lightyear_inputs);
    }
}

fn capture_lightyear_inputs(
    actions: Query<&ActionState<PlayerInputAction>, With<PlayerControlled>>,
    camera: Query<&Transform, With<FpsCam>>,
    sync_time: Option<Res<SyncTime>>,
    mut snapshot: ResMut<LightyearInputSnapshot>,
) {
    let Ok(action_state) = actions.get_single() else {
        return;
    };

    let camera_transform = camera.get_single().copied().unwrap_or_default();
    let tick_ms = sync_time.as_ref().map(|s| s.now_synced() as u64).unwrap_or(0);

    snapshot.tick_ms = tick_ms;
    snapshot.action_mask = action_mask_from_leafwing(action_state);
    snapshot.camera = camera_transform;
}
