use bevy::prelude::*;
use bevy_renet::renet::{ClientId, RenetServer};
use shared::messages::{
    ClientToServerPlayerInput, PlayerId, ServerToClientMessage, ServerToClientPlayerUpdate,
};
use shared::physics::{apply_player_input_to_components, LinearVelocity, MovementMode, OnGround};
use std::collections::HashMap;

use super::dispatcher::NetworkPlayer;
use super::extensions::SendGameMessageExtension;

// Re-export from shared for backward compatibility
pub use shared::DEFAULT_SPAWN_POSITION;

// =============================================================================
// Note: Ground state and stepped block updates now use shared systems from
// shared::physics::ground_detection. See dispatcher.rs for system registration:
// - update_ground_state_system::<NetworkPlayer>
// - update_stepped_block_system::<NetworkPlayer, VoxelWorld>
// =============================================================================

/// The position the client predicted when sending its input.
///
/// The client runs local physics prediction for responsive gameplay. When sending
/// inputs to the server, it includes its predicted position. The server stores this
/// for diagnostic purposes (comparing client prediction vs server authority).
#[derive(Component, Debug, Clone, Default)]
pub struct ClientPredictedPosition(pub Vec3);

#[derive(Debug, Clone)]
pub struct ServerPlayer {
    pub id: PlayerId,
    pub name: String,
    pub last_input_processed: u64,
    pub is_authenticated: bool,
}

impl ServerPlayer {
    pub fn new(id: PlayerId) -> Self {
        Self {
            id,
            name: format!("Player-{}", id),
            last_input_processed: 0,
            is_authenticated: false,
        }
    }
}

#[derive(Resource, Default)]
pub struct PlayerRegistry {
    pub players: HashMap<PlayerId, ServerPlayer>,
}

impl PlayerRegistry {
    /// Add a new player to the registry
    pub fn add_player(&mut self, client_id: ClientId) {
        let player = ServerPlayer::new(client_id);
        info!("Adding player {} to registry", client_id);
        self.players.insert(client_id, player);
    }

    /// Remove a player from the registry
    pub fn remove_player(&mut self, client_id: ClientId) {
        info!("Removing player {} from registry", client_id);
        self.players.remove(&client_id);
    }

    /// Get a mutable reference to a player
    pub fn get_player_mut(&mut self, client_id: ClientId) -> Option<&mut ServerPlayer> {
        self.players.get_mut(&client_id)
    }

    /// Get an immutable reference to a player
    pub fn get_player(&self, client_id: ClientId) -> Option<&ServerPlayer> {
        self.players.get(&client_id)
    }

    /// Check if a player is authenticated
    pub fn is_authenticated(&self, client_id: ClientId) -> bool {
        self.players
            .get(&client_id)
            .map(|p| p.is_authenticated)
            .unwrap_or(false)
    }
}

#[derive(Message, Debug)]
pub struct PlayerInputsEvent {
    pub client_id: ClientId,
    pub input: ClientToServerPlayerInput,
}

/// Server-authoritative player input handling system.
///
/// This system receives player inputs from clients, computes desired velocity,
/// and sets it on the player's LinearVelocity component. Avian3d will then
/// integrate the velocity and resolve collisions against chunk colliders.
///
/// Note: MovementMode is stored directly as a component, and velocity is
/// stored in LinearVelocity (no separate ServerPhysicsState needed).
pub fn handle_player_inputs_system(
    mut events: MessageReader<PlayerInputsEvent>,
    mut registry: ResMut<PlayerRegistry>,
    mut player_query: Query<(
        &NetworkPlayer,
        &mut LinearVelocity,
        &mut MovementMode,
        &mut ClientPredictedPosition,
        &OnGround,
    )>,
) {
    for ev in events.read() {
        let Some(player) = registry.get_player_mut(ev.client_id) else {
            warn!("Received input from unknown player {}", ev.client_id);
            continue;
        };

        if !player.is_authenticated {
            debug!(
                "Ignoring input from unauthenticated player {}",
                ev.client_id
            );
            continue;
        }

        let Some((_, mut linear_velocity, mut movement_mode, mut predicted_pos, on_ground)) =
            player_query
                .iter_mut()
                .find(|(np, _, _, _, _)| np.client_id == ev.client_id)
        else {
            warn!(
                "No ECS entity found for authenticated player {}",
                ev.client_id
            );
            continue;
        };

        predicted_pos.0 = ev.input.predicted_position;

        // Drop stale/duplicate inputs based on last processed timestamp.
        if ev.input.time_ms <= player.last_input_processed {
            continue;
        }

        let delta_seconds = ev.input.delta_ms as f32 / 1000.0;
        apply_player_input_to_components(
            &mut linear_velocity,
            &mut movement_mode,
            on_ground.0,
            &ev.input.inputs,
            &ev.input.camera,
            delta_seconds,
        );

        player.last_input_processed = ev.input.time_ms;
    }
}

pub fn broadcast_player_updates_system(
    registry: Res<PlayerRegistry>,
    mut server: ResMut<RenetServer>,
    player_query: Query<(&NetworkPlayer, &Transform, &LinearVelocity, &MovementMode)>,
) {
    // Only broadcast authenticated players
    for player in registry.players.values().filter(|p| p.is_authenticated) {
        // Get position, orientation, velocity, and movement mode from ECS components
        let (position, orientation, velocity, movement_mode) = player_query
            .iter()
            .find(|(np, _, _, _)| np.client_id == player.id)
            .map(|(_, t, lv, mm)| (t.translation, t.rotation, lv.0, *mm))
            .unwrap_or((
                DEFAULT_SPAWN_POSITION,
                Quat::IDENTITY,
                Vec3::ZERO,
                MovementMode::Walking,
            ));

        let update = ServerToClientPlayerUpdate {
            id: player.id,
            position,
            velocity,
            orientation,
            movement_mode,
            last_ack_time: player.last_input_processed,
            inventory: Box::new([]), // TODO: Implement inventory sync
        };

        server.broadcast_game_message(ServerToClientMessage::PlayerUpdate(update));
    }
}
