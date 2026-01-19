use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use serde::{Deserialize, Serialize};
use shared::world::pos::pos2d::ColPos;
use shared::world::pos::PlayerCol;
use shared::world::{realm::Realm, BlockRayCastHit};

use super::block_action::BlockActionPlugin;
use shared::net::lightyear_inputs::PlayerInputAction;
pub const HOTBAR_SLOTS: usize = 8;

pub struct PlayerPlugin;

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, SystemSet)]
pub struct PlayerSpawn;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BlockActionPlugin)
            .add_plugins(InputManagerPlugin::<PlayerInputAction>::default())
            .add_systems(Update, update_player_col);
    }
}

/// Marker component for the locally controlled player entity.
/// Added by lightyear_client when the character is replicated and predicted.
#[derive(Component)]
pub struct PlayerControlled;

/// The block the player is currently targeting (raycast result).
#[derive(Component)]
pub struct TargetBlock(pub Option<BlockRayCastHit>);

#[derive(Serialize, Deserialize)]
struct KeyBindsConfig {
    forward: KeyCode,
    backward: KeyCode,
    left: KeyCode,
    right: KeyCode,
    jump: KeyCode,
    crouch: KeyCode,
    toggle_fly: KeyCode,
    hit: MouseButton,
    modify: MouseButton,
}

impl Default for KeyBindsConfig {
    fn default() -> Self {
        Self {
            forward: KeyCode::KeyW,
            backward: KeyCode::KeyS,
            left: KeyCode::KeyA,
            right: KeyCode::KeyD,
            jump: KeyCode::Space,
            crouch: KeyCode::ShiftLeft,
            toggle_fly: KeyCode::F1,
            hit: MouseButton::Left,
            modify: MouseButton::Right,
        }
    }
}

/// Load the configured input map from key_bindings.toml or use defaults.
pub fn configured_input_map() -> InputMap<PlayerInputAction> {
    let cfg: KeyBindsConfig = confy::load_path("key_bindings.toml").unwrap_or_default();

    let mut map = InputMap::default();
    map.insert_dual_axis(
        PlayerInputAction::Move,
        VirtualDPad::new(cfg.forward, cfg.backward, cfg.left, cfg.right),
    );
    map.insert(PlayerInputAction::Jump, cfg.jump);
    map.insert(PlayerInputAction::Crouch, cfg.crouch);
    map.insert(PlayerInputAction::ToggleFly, cfg.toggle_fly);
    map.insert(PlayerInputAction::Hit, cfg.hit);
    map.insert(PlayerInputAction::Modify, cfg.modify);

    // Keep sensible gamepad defaults alongside configurable keyboard/mouse.
    map.insert_dual_axis(PlayerInputAction::Move, GamepadStick::LEFT);
    map.insert(PlayerInputAction::Jump, GamepadButton::South);
    map.insert(PlayerInputAction::Crouch, GamepadButton::East);
    map.insert(PlayerInputAction::ToggleFly, GamepadButton::West);

    map
}

/// Updates the PlayerCol component when the player moves to a different chunk column.
/// This system ensures LOD remeshing is triggered when the player's position changes.
fn update_player_col(
    mut commands: Commands,
    mut player_query: Query<
        (Entity, &Transform, &Realm, Option<&mut PlayerCol>),
        With<PlayerControlled>,
    >,
) {
    let Ok((entity, transform, realm, player_col_opt)) = player_query.single_mut() else {
        return;
    };

    let new_col = ColPos::from((transform.translation, *realm));

    match player_col_opt {
        Some(mut player_col) => {
            // Update only if the column changed
            if player_col.0 != new_col {
                player_col.0 = new_col;
            }
        }
        None => {
            // Initial assignment
            commands.entity(entity).insert(PlayerCol(new_col));
        }
    }
}

