//! Lightyear server bootstrap: spins up a netcode UDP listener and registers
//! our replication protocol. Feature-gated behind `lightyear-net` so Renet
//! remains the default path while this matures.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use bevy::color::palettes::css;
use bevy::input::InputPlugin;
use bevy::log::info;
use bevy::prelude::*;
use bevy::transform::TransformPlugin;
use leafwing_input_manager::prelude::ActionState;
use lightyear::connection::client::Connected;
use lightyear::netcode::{NetcodeServer, PRIVATE_KEY_BYTES};
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use shared::block::Block;
use shared::items::{new_inventory, InventoryTrait, Item, Stack};
use shared::items::item_slots::ItemHolder;
use shared::messages::ActionMask;
use shared::net::lightyear_inputs::{action_mask_from_leafwing, PlayerInputAction};
use shared::net::lightyear_protocol::{CameraOrientation, CharacterMarker, LightyearProtocolPlugin, PlayerColor};
use shared::physics::{
    apply_player_input_to_physics, DynamicPlayerPhysicsBundle, Grounded, LinearVelocity,
    MovementMode,
};
use shared::world::realm::Realm;
use shared::{DEFAULT_SPAWN_POSITION, PROTOCOL_ID, TICKS_PER_SECOND};

/// Interval at which the server sends replication updates to clients.
const SEND_INTERVAL: Duration = Duration::from_millis(50);

/// Development private key for netcode (all zeros - NOT for production).
const LIGHTYEAR_DEV_PRIVATE_KEY: [u8; PRIVATE_KEY_BYTES] = [0; PRIVATE_KEY_BYTES];

/// Server networking settings for the Lightyear path.
#[derive(Resource, Debug, Clone)]
pub struct LightyearServerConfig {
    pub bind_addr: SocketAddr,
    pub protocol_id: u64,
    pub private_key: [u8; PRIVATE_KEY_BYTES],
    pub max_clients: usize,
    pub keep_alive_hz: f64,
    pub client_timeout_secs: i32,
    pub num_disconnect_packets: usize,
}

impl Default for LightyearServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 5000),
            protocol_id: PROTOCOL_ID,
            private_key: LIGHTYEAR_DEV_PRIVATE_KEY,
            max_clients: 64,
            keep_alive_hz: 10.0,
            client_timeout_secs: 10,
            num_disconnect_packets: 10,
        }
    }
}

/// Latest inputs received from clients (placeholder until Lightyear transport is wired).
#[derive(Resource, Default, Debug, Clone)]
pub struct LightyearServerInbox {
    pub pending: Vec<(PeerId, u64, ActionMask)>,
}

pub struct LightyearServerPlugin;

impl Plugin for LightyearServerPlugin {
    fn build(&self, app: &mut App) {
        // Bevy basics Lightyear expects but our headless stack may omit.
        if !app.is_plugin_added::<TransformPlugin>() {
            app.add_plugins(TransformPlugin);
        }
        if !app.is_plugin_added::<InputPlugin>() {
            app.add_plugins(InputPlugin);
        }

        // Core Lightyear server stack and our protocol registration.
        app.add_plugins(ServerPlugins {
            tick_duration: Duration::from_secs_f64(1.0 / TICKS_PER_SECOND as f64),
        });
        app.add_plugins(LightyearProtocolPlugin);

        // Physics integration with lightyear (handles rollback, Position<->Transform sync).
        app.add_plugins(shared::net::LightyearPhysicsPlugin);

        app.init_resource::<LightyearServerConfig>()
            .init_resource::<LightyearServerInbox>()
            .add_systems(Startup, spawn_lightyear_server)
            // Apply character actions from replicated inputs.
            .add_systems(FixedUpdate, handle_character_actions)
            // Register observers for client connection lifecycle.
            .add_observer(handle_new_client)
            .add_observer(handle_connected);
    }
}

/// Spawn the netcode-backed Lightyear server and kick off the transport.
fn spawn_lightyear_server(
    mut commands: Commands,
    config: Res<LightyearServerConfig>,
    existing: Query<Entity, With<NetcodeServer>>,
) {
    // Only spin up the listener once even if the plugin is hot-reloaded.
    if !existing.is_empty() {
        return;
    }

    let server_entity = commands
        .spawn((
            Name::new("Lightyear Netcode Server"),
            NetcodeServer::new(NetcodeConfig {
                protocol_id: config.protocol_id,
                private_key: config.private_key,
                keep_alive_send_rate: 1.0 / config.keep_alive_hz,
                client_timeout_secs: config.client_timeout_secs,
                num_disconnect_packets: config.num_disconnect_packets,
            }),
            LocalAddr(config.bind_addr),
            ServerUdpIo::default(),
        ))
        .id();

    // Start listening immediately.
    commands.trigger(Start {
        entity: server_entity,
    });

    info!("Lightyear server listening on {}", config.bind_addr);
}

/// Add the ReplicationSender component to new clients.
fn handle_new_client(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert(ReplicationSender::new(
            SEND_INTERVAL,
            SendUpdatesMode::SinceLastAck,
            false,
        ));
}

/// Spawn a player character entity when a client finishes connecting.
fn handle_connected(
    trigger: On<Add, Connected>,
    query: Query<&RemoteId, With<ClientOf>>,
    mut commands: Commands,
    character_query: Query<Entity, With<CharacterMarker>>,
) {
    let Ok(client_id) = query.get(trigger.entity) else {
        return;
    };
    let client_id = client_id.0;
    info!("Client connected with client-id {client_id:?}. Spawning character entity.");

    // Track the number of characters to pick colors and starting positions.
    let num_characters = character_query.iter().count();

    // Pick color for player.
    let available_colors = [
        css::LIMEGREEN,
        css::PINK,
        css::YELLOW,
        css::AQUA,
        css::CRIMSON,
        css::GOLD,
        css::ORANGE_RED,
        css::SILVER,
        css::SALMON,
        css::YELLOW_GREEN,
        css::WHITE,
        css::RED,
    ];
    let color = available_colors[num_characters % available_colors.len()];

    // Spawn player at the default spawn position with a slight offset per player.
    let offset = Vec3::new(num_characters as f32 * 2.0, 0.0, 0.0);
    let spawn_position = DEFAULT_SPAWN_POSITION + offset;

    // Spawn the character with ActionState. The client will add their own InputMap.
    // Create initial inventory with starter items.
    let mut inventory = new_inventory::<8>(); // 8-slot hotbar
    inventory.try_add(Stack::Some(Item::Block(Block::Smelter), 1));
    inventory.try_add(Stack::Some(Item::Coal, 20));
    inventory.try_add(Stack::Some(Item::IronOre, 50));

    let character = commands
        .spawn((
            Name::new(format!("Player-{}", client_id)),
            // Leafwing input state - server needs this to receive replicated inputs.
            ActionState::<PlayerInputAction>::default(),
            // Camera orientation - replicated from client to server for directional movement.
            CameraOrientation::default(),
            // Player inventory - server-authoritative, replicated to clients.
            ItemHolder::Inventory(inventory),
            // Replication configuration.
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
            ControlledBy {
                owner: trigger.entity,
                lifetime: Default::default(),
            },
            // Physics bundle for server-side simulation.
            DynamicPlayerPhysicsBundle::from_transform(
                &Transform::from_translation(spawn_position),
                Realm::Overworld,
            ),
            // Marker and visual components for replication.
            CharacterMarker,
            PlayerColor(color.into()),
            Realm::Overworld,
        ))
        .id();

    info!("Created entity {character:?} for client {client_id:?}");
}

/// Apply character actions from replicated inputs to all characters.
fn handle_character_actions(
    time: Res<Time>,
    mut player_query: Query<(
        &ActionState<PlayerInputAction>,
        &CameraOrientation,
        &mut LinearVelocity,
        &mut MovementMode,
        &Grounded,
    )>,
) {
    let delta_seconds = time.delta_secs();

    for (action_state, camera_orientation, mut linear_velocity, mut movement_mode, grounded) in &mut player_query {
        // Convert leafwing action state to our ActionMask for the existing physics system.
        let action_mask = action_mask_from_leafwing(action_state);

        // Use the replicated camera orientation for directional movement.
        let camera_transform = camera_orientation.to_transform();

        apply_player_input_to_physics(
            &mut linear_velocity,
            &mut movement_mode,
            grounded.0,
            &action_mask,
            &camera_transform,
            delta_seconds,
        );
    }
}
