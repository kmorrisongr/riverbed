use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use bevy::log::info;
use bevy::prelude::*;
use bevy::transform::TransformPlugin;
use leafwing_input_manager::prelude::*;
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::netcode::{NetcodeClient, PRIVATE_KEY_BYTES};
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use shared::messages::ActionMask;
use shared::net::lightyear_inputs::{action_mask_from_leafwing, PlayerInputAction};
use shared::net::lightyear_protocol::{
    CameraOrientation, CharacterMarker, LightyearProtocolPlugin, SelectedHotbarSlot,
};
use shared::physics::{
    apply_player_input_to_physics, DynamicPlayerPhysicsBundle, Grounded, LinearVelocity,
    MovementMode,
};
use shared::world::realm::Realm;
use shared::{PROTOCOL_ID, TICKS_PER_SECOND};

use crate::agents::player::configured_input_map;
use crate::agents::{Crouching, PlayerControlled, TargetBlock};
use crate::network::setup::{CurrentPlayerProfile, TargetServer};
use crate::render::FpsCam;
use crate::sounds::{on_item_get, BlockSoundCD, FootstepCD};
use crate::ui::SelectedHotbarSlot as UiSelectedHotbarSlot;

const LIGHTYEAR_DEV_PRIVATE_KEY: [u8; PRIVATE_KEY_BYTES] = [0; PRIVATE_KEY_BYTES];

#[derive(Resource, Debug, Clone)]
pub struct LightyearClientConfig {
    pub server_addr: SocketAddr,
    pub client_addr: SocketAddr,
    pub protocol_id: u64,
    pub private_key: [u8; PRIVATE_KEY_BYTES],
    pub client_id: u64,
    pub keep_alive_hz: f64,
    pub client_timeout_secs: i32,
    pub token_expire_secs: i32,
    pub num_disconnect_packets: usize,
}

impl Default for LightyearClientConfig {
    fn default() -> Self {
        Self {
            server_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000),
            client_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
            protocol_id: PROTOCOL_ID,
            private_key: LIGHTYEAR_DEV_PRIVATE_KEY,
            client_id: 1,
            keep_alive_hz: 10.0,
            client_timeout_secs: 10,
            token_expire_secs: 30,
            num_disconnect_packets: 10,
        }
    }
}

#[derive(Resource, Default, Debug, Clone)]
pub struct LightyearInputSnapshot {
    pub action_mask: ActionMask,
    pub camera: Transform,
}

pub struct LightyearClientPlugin;

impl Plugin for LightyearClientPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TransformPlugin>() {
            app.add_plugins(TransformPlugin);
        }

        app.add_plugins(ClientPlugins {
            tick_duration: Duration::from_secs_f64(1.0 / TICKS_PER_SECOND as f64),
        });
        app.add_plugins(LightyearProtocolPlugin);

        app.add_plugins(shared::net::LightyearPhysicsPlugin);

        app.init_resource::<LightyearClientConfig>()
            .init_resource::<LightyearInputSnapshot>()
            .add_systems(Startup, spawn_lightyear_client)
            .add_systems(PreUpdate, capture_lightyear_inputs)
            .add_systems(Update, handle_new_character)
            .add_systems(Update, sync_camera_orientation)
            .add_systems(Update, sync_selected_hotbar_slot)
            .add_systems(FixedUpdate, handle_character_actions);
    }
}

/// Spawn a Lightyear netcode client and kick off the connection.
fn spawn_lightyear_client(
    mut commands: Commands,
    config: Res<LightyearClientConfig>,
    target_server: Option<Res<TargetServer>>,
    player_profile: Option<Res<CurrentPlayerProfile>>,
    existing: Query<Entity, With<NetcodeClient>>,
) {
    if !existing.is_empty() {
        return;
    }

    // Wait until a target server address is known (local or external) to avoid
    // attempting a connection to the default and timing out.
    let Some(server_addr) = target_server.as_ref().and_then(|t| t.address) else {
        return;
    };
    let client_id = player_profile
        .as_ref()
        .map(|p| p.id)
        .unwrap_or(config.client_id);

    let auth = Authentication::Manual {
        server_addr,
        client_id,
        private_key: config.private_key,
        protocol_id: config.protocol_id,
    };

    let netcode_config = NetcodeConfig {
        num_disconnect_packets: config.num_disconnect_packets,
        keepalive_packet_send_rate: 1.0 / config.keep_alive_hz,
        client_timeout_secs: config.client_timeout_secs,
        token_expire_secs: config.token_expire_secs,
    };

    let client_entity = commands
        .spawn((
            Name::new("Lightyear Netcode Client"),
            Client::default(),
            Link::new(None),
            LocalAddr(config.client_addr),
            PeerAddr(server_addr),
            ReplicationReceiver::default(),
            PredictionManager::default(),
            NetcodeClient::new(auth, netcode_config).expect("failed to build netcode client"),
            UdpIo::default(),
        ))
        .id();

    commands.trigger(Connect {
        entity: client_entity,
    });

    info!(
        "Lightyear client connecting to {} as {}",
        server_addr, client_id
    );
}

fn capture_lightyear_inputs(
    actions: Query<&ActionState<PlayerInputAction>, With<PlayerControlled>>,
    camera: Query<&Transform, With<FpsCam>>,
    mut snapshot: ResMut<LightyearInputSnapshot>,
) {
    let Ok(action_state) = actions.single() else {
        return;
    };

    let camera_transform = camera.single().copied().unwrap_or_default();

    snapshot.action_mask = action_mask_from_leafwing(action_state);
    snapshot.camera = camera_transform;
}

fn handle_new_character(
    mut commands: Commands,
    character_query: Query<(Entity, Has<Controlled>), (Added<Predicted>, With<CharacterMarker>)>,
) {
    for (entity, is_controlled) in &character_query {
        if is_controlled {
            info!("Setting up controlled and predicted character {entity:?}");
            commands
                .entity(entity)
                .insert((
                    configured_input_map(),
                    ActionState::<PlayerInputAction>::default(),
                    PlayerControlled,
                    CameraOrientation::default(),
                    SelectedHotbarSlot::default(),
                    TargetBlock(None),
                    Crouching(false),
                    Visibility::default(),
                ))
                .insert((FootstepCD(0.), BlockSoundCD(0.)))
                .insert(SpatialListener::new(0.3))
                .observe(on_item_get);
        } else {
            info!("Remote character predicted for us: {entity:?}");
            commands.entity(entity).insert(Visibility::default());
        }

        info!(?entity, "Adding physics bundle to character");
        commands
            .entity(entity)
            .insert((DynamicPlayerPhysicsBundle::default(), Realm::Overworld));
    }
}

fn sync_camera_orientation(
    camera_query: Query<&FpsCam>,
    mut character_query: Query<&mut CameraOrientation, (With<Controlled>, With<CharacterMarker>)>,
) {
    let Ok(fps_cam) = camera_query.single() else {
        return;
    };
    let Ok(mut cam_orientation) = character_query.single_mut() else {
        return;
    };

    cam_orientation.yaw = fps_cam.yaw;
    cam_orientation.pitch = fps_cam.pitch;
}

fn sync_selected_hotbar_slot(
    ui_slot: Option<Res<UiSelectedHotbarSlot>>,
    mut character_query: Query<&mut SelectedHotbarSlot, (With<Controlled>, With<CharacterMarker>)>,
) {
    let Some(ui_slot) = ui_slot else {
        return;
    };
    let Ok(mut hotbar_slot) = character_query.single_mut() else {
        return;
    };

    let new_slot = ui_slot.0 as u8;
    if hotbar_slot.0 != new_slot {
        hotbar_slot.0 = new_slot;
    }
}

fn handle_character_actions(
    time: Res<Time>,
    camera_query: Query<&FpsCam>,
    mut player_query: Query<
        (
            &ActionState<PlayerInputAction>,
            Option<&CameraOrientation>,
            Has<Controlled>,
            &mut LinearVelocity,
            &mut MovementMode,
            &Grounded,
        ),
        With<Predicted>,
    >,
) {
    let local_camera = camera_query.single().ok();
    let delta_seconds = time.delta_secs();

    for (
        action_state,
        camera_orientation,
        is_controlled,
        mut linear_velocity,
        mut movement_mode,
        grounded,
    ) in &mut player_query
    {
        let action_mask = action_mask_from_leafwing(action_state);

        let camera_transform = if is_controlled {
            local_camera
                .map(|cam| {
                    Transform::from_rotation(
                        Quat::from_rotation_y(cam.yaw) * Quat::from_rotation_x(cam.pitch),
                    )
                })
                .unwrap_or_default()
        } else {
            camera_orientation
                .map(|co| co.to_transform())
                .unwrap_or_default()
        };

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
