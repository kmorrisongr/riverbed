//! Server-authoritative reconciliation for client-side prediction.
//!
//! This module implements the client-side prediction and reconciliation system
//! for a server-authoritative networking model. The server is the single source
//! of truth (SSOT) for player positions.
//!
//! With avian3d physics, collision resolution is handled by the physics engine,
//! so we can't simply replay inputs to predict position. Instead, we:
//! 1. Client predicts movement locally using avian3d for responsive gameplay
//! 2. Client sends inputs to server  
//! 3. Server simulates authoritatively and broadcasts position updates
//! 4. Client receives server state and smoothly corrects toward it

use bevy::prelude::*;
use shared::messages::ServerToClientPlayerUpdate;
use shared::physics::{sync_movement_mode_components, FreeFly, LinearVelocity, MovementMode};

use crate::agents::PlayerControlled;
use crate::network::CurrentPlayerProfile;
use shared::net::input_history::InputHistory;

/// Threshold for position correction. If the difference between predicted and actual
/// client position is less than this, we don't correct (to avoid jitter).
pub const POSITION_ERROR_IGNORE_THRESHOLD_METERS: f32 = 0.05;

/// Maximum allowed position error before we force a hard snap (teleport).
/// Below this threshold, we interpolate smoothly.
pub const POSITION_ERROR_HARD_SNAP_THRESHOLD_METERS: f32 = 2.0;

/// Interpolation factor for smooth corrections (0.0 = no correction, 1.0 = instant snap).
pub const CORRECTION_LERP_FACTOR: f32 = 0.3;

/// Plugin for client-side reconciliation
pub struct ReconciliationPlugin;

impl Plugin for ReconciliationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, reconcile_player_state);
    }
}

/// System that reconciles the local player's state with server updates.
///
/// Since avian3d handles collision resolution, we can't replay inputs to predict
/// position. Instead, we smoothly correct the client's position toward the
/// server's authoritative position.
pub fn reconcile_player_state(
    mut ev_update: MessageReader<ServerToClientPlayerUpdate>,
    mut player_query: Query<
        (&mut Transform, &mut LinearVelocity, Option<&FreeFly>),
        With<PlayerControlled>,
    >,
    mut commands: Commands,
    player_entity: Query<Entity, With<PlayerControlled>>,
    current_player: Res<CurrentPlayerProfile>,
    mut input_history: ResMut<InputHistory>,
) {
    for event in ev_update.read() {
        // Only process updates for our own player
        if event.id != current_player.id {
            continue;
        }

        // Remove acknowledged inputs (server has processed these)
        let acked_count = input_history.ack_until(event.last_ack_time);

        if acked_count > 0 {
            debug!(
                "Acknowledged {} inputs (remaining: {})",
                acked_count,
                input_history.unacknowledged.len()
            );
        }

        let Ok((mut transform, mut linear_velocity, free_fly_opt)) = player_query.single_mut()
        else {
            warn!("No local player entity found for reconciliation");
            continue;
        };

        let Ok(entity) = player_entity.single() else {
            continue;
        };

        // Update movement mode if it differs
        let server_is_flying = event.movement_mode == MovementMode::Flying;
        let client_is_flying = free_fly_opt.is_some();

        if server_is_flying != client_is_flying {
            sync_movement_mode_components(&mut commands, entity, event.movement_mode, client_is_flying);
            info!("Movement mode corrected: flying={}", server_is_flying);
        }

        // Calculate position error
        let position_error = (event.position - transform.translation).length();

        if position_error < POSITION_ERROR_IGNORE_THRESHOLD_METERS {
            // Prediction is accurate - no position correction needed
            // Just sync velocity to keep future predictions accurate
            linear_velocity.0 = event.velocity.into();
            continue;
        }

        if position_error > POSITION_ERROR_HARD_SNAP_THRESHOLD_METERS {
            // Large error - hard snap to server position
            warn!(
                "Large position error ({:.2}m), hard snapping to server position",
                position_error
            );
            transform.translation = event.position;
            linear_velocity.0 = event.velocity.into();
        } else {
            // Small error - smoothly correct toward server position
            debug!(
                "Position error: {:.3}m, applying smooth correction",
                position_error,
            );
            transform.translation = transform
                .translation
                .lerp(event.position, CORRECTION_LERP_FACTOR);
            linear_velocity.0 = event.velocity.into();
        }
    }
}
