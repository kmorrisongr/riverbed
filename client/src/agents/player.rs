use crate::sounds::{on_item_get, BlockSoundCD, FootstepCD};
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use serde::{Deserialize, Serialize};
use shared::physics::DynamicPlayerPhysicsBundle;
use shared::world::pos::pos2d::ColPos;
use shared::world::pos::PlayerCol;
use shared::{
    block::Block,
    items::{item_slots::ItemHolder, new_inventory, InventoryTrait, Item, Stack},
    world::{realm::Realm, BlockRayCastHit},
    DEFAULT_SPAWN_POSITION,
};

use super::{block_action::BlockActionPlugin, Crouching};
use shared::net::lightyear_inputs::PlayerInputAction;
pub const HOTBAR_SLOTS: usize = 8;

pub struct PlayerPlugin;

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, SystemSet)]
pub struct PlayerSpawn;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BlockActionPlugin)
            .add_plugins(InputManagerPlugin::<PlayerInputAction>::default())
            .add_systems(
                Startup,
                (spawn_player, ApplyDeferred).chain().in_set(PlayerSpawn),
            )
            .add_systems(Update, update_player_col);
    }
}

#[derive(Component)]
pub struct PlayerControlled;

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

fn configured_input_map() -> InputMap<PlayerInputAction> {
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

pub fn spawn_player(mut commands: Commands) {
    let realm = Realm::Overworld;
    let mut inventory = new_inventory::<HOTBAR_SLOTS>();
    inventory.try_add(Stack::Some(Item::Block(Block::Smelter), 1));
    inventory.try_add(Stack::Some(Item::Coal, 20));
    inventory.try_add(Stack::Some(Item::IronOre, 50));
    let transform = Transform {
        translation: DEFAULT_SPAWN_POSITION,
        ..default()
    };
    commands
        .spawn((
            transform,
            Visibility::default(),
            realm,
            DynamicPlayerPhysicsBundle::from_transform(&transform, realm),
            TargetBlock(None),
            ItemHolder::Inventory(inventory),
            PlayerControlled,
            Crouching(false),
        ))
        .insert(SpatialListener::new(0.3))
        .insert((FootstepCD(0.), BlockSoundCD(0.)))
        // Leafwing bundle was removed; insert components directly
        .insert(configured_input_map())
        .insert(ActionState::<PlayerInputAction>::default())
        .observe(on_item_get);
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
