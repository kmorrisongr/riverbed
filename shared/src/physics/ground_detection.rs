//! Ground detection using avian3d collision contacts.
//!
//! This module provides components and systems for ground state tracking:
//! - `Grounded`: Whether an entity is standing on a surface (from avian3d contacts)
//! - `BlockBeneathFeet`: Which block type the entity is standing on (for surface effects)
//!
//! The generic systems `sync_grounded_state` and `sync_block_beneath_feet`
//! can be used by both client and server with their respective marker components.

use avian3d::prelude::Collisions;
use bevy::prelude::*;

use crate::block::Block;
use crate::physics::{PLAYER_CAPSULE_HEIGHT, PLAYER_CAPSULE_RADIUS, PLAYER_QUERY_BOUNDS};
use crate::world::block_access::BlockAccess;
use crate::world::pos::pos3d::BlockPos;
use crate::world::realm::Realm;

/// Minimum Y component of contact normal to count as "ground".
/// 0.7 corresponds to approximately a 45-degree slope.
pub const MIN_GROUND_NORMAL_Y: f32 = 0.7;

/// Component that tracks whether an entity is on the ground.
/// Updated each frame from avian3d collision data.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Grounded(pub bool);

/// Component that tracks which block type an entity is standing on.
///
/// This is used for gameplay effects like:
/// - Footstep sounds (different sounds for stone vs dirt vs wood)
/// - Surface-specific friction (ice is slippery, soul sand is slow)
/// - Visual effects (dust particles on sand, etc.)
///
/// Updated by querying the voxel world directly (not avian3d contacts),
/// since we need to know the actual block type, not just collision geometry.
#[derive(Component, Debug, Clone, Copy)]
pub struct BlockBeneathFeet(pub Block);

impl Default for BlockBeneathFeet {
    fn default() -> Self {
        Self(Block::Air)
    }
}

/// Get block positions beneath a capsule-centered player footprint.
fn footprint_block_positions(
    capsule_center: Vec3,
    realm: Realm,
    footprint_size: Vec3,
) -> impl Iterator<Item = BlockPos> {
    // Determine Y level just below feet (assuming pos is capsule center)
    let feet_y = capsule_center.y - (PLAYER_CAPSULE_HEIGHT / 2.0 + PLAYER_CAPSULE_RADIUS);
    let y = (feet_y - 0.01).floor() as i32;

    // Center the query area on the position
    let x_start = (capsule_center.x - footprint_size.x / 2.0).floor() as i32;
    let x_end = (capsule_center.x + footprint_size.x / 2.0).floor() as i32;
    let z_start = (capsule_center.z - footprint_size.z / 2.0).floor() as i32;
    let z_end = (capsule_center.z + footprint_size.z / 2.0).floor() as i32;

    (x_start..=x_end)
        .flat_map(move |x| (z_start..=z_end).map(move |z| (x, z)))
        .map(move |(x, z)| BlockPos { x, y, z, realm })
}

/// Find the solid block closest to the entity's feet.
///
/// Expects the position to be the center of a capsule collider and queries using the
/// provided footprint size around the feet.
pub fn find_block_beneath_feet<W: BlockAccess>(
    world: &W,
    capsule_center: Vec3,
    realm: Realm,
    footprint_size: Vec3,
) -> Block {
    let mut closest_block = Block::Air;
    let mut min_dist_sq = f32::INFINITY;

    for block_pos in footprint_block_positions(capsule_center, realm, footprint_size) {
        let block = world.get_block_safe(block_pos);
        if block.is_traversable() {
            continue;
        }

        // Horizontal distance squared from center
        let dist_sq = (capsule_center.x - (block_pos.x as f32 + 0.5)).powi(2)
            + (capsule_center.z - (block_pos.z as f32 + 0.5)).powi(2);

        if dist_sq < min_dist_sq {
            min_dist_sq = dist_sq;
            closest_block = block;
        }
    }
    closest_block
}

/// Check if any contact normal indicates standing on ground.
///
/// A contact is considered "ground" if:
/// - The contact is currently touching (not just a sensor overlap)
/// - The contact normal has a Y component > MIN_GROUND_NORMAL_Y
///
/// The normal points from first shape to second, so we need to check
/// both directions depending on which entity we're querying for.
pub fn check_grounded_from_collisions(collisions: &Collisions, entity: Entity) -> bool {
    for contact_pair in collisions.collisions_with(entity) {
        // Skip if not actually touching
        if !contact_pair.is_touching() {
            continue;
        }

        // Determine if we're the first or second body in the pair
        let is_first = contact_pair.body1 == Some(entity) || contact_pair.collider1 == entity;

        for manifold in &contact_pair.manifolds {
            // Normal points from first to second shape
            // If we're the first body, a negative Y normal means we're being pushed up (standing on ground)
            // If we're the second body, a positive Y normal means we're being pushed up
            let effective_y = if is_first {
                -manifold.normal.y
            } else {
                manifold.normal.y
            };

            if effective_y > MIN_GROUND_NORMAL_Y {
                return true;
            }
        }
    }
    false
}

// =============================================================================
// Generic Systems for Ground State Updates
// =============================================================================
// These systems can be used by both client and server by specifying the
// appropriate marker component (e.g., PlayerControlled on client, NetworkPlayer
// on server).
// =============================================================================

/// Syncs the `Grounded` component from avian3d collision contacts.
///
/// This system queries all entities with the given marker component `M` and
/// updates their `Grounded` state based on collision contacts. Should run
/// before movement input processing so jump detection uses current frame's state.
///
/// # Type Parameters
/// - `M`: Marker component to filter which entities to update (e.g., `PlayerControlled`)
pub fn sync_grounded_state<M: Component>(
    collisions: Collisions,
    mut query: Query<(Entity, &mut Grounded), With<M>>,
) {
    for (entity, mut grounded) in query.iter_mut() {
        grounded.0 = check_grounded_from_collisions(&collisions, entity);
    }
}

/// Syncs the `BlockBeneathFeet` component by querying the voxel world.
///
/// This system queries all entities with the given marker component `M` and
/// updates their `BlockBeneathFeet` block type by checking the voxel world directly.
/// Used for footstep sounds and surface-specific effects.
///
/// # Type Parameters
/// - `M`: Marker component to filter which entities to update
/// - `W`: World resource that implements `BlockAccess` (e.g., `ClientWorldMap`, `VoxelWorld`)
pub fn sync_block_beneath_feet<M: Component, W: BlockAccess + Resource>(
    world: Option<Res<W>>,
    mut query: Query<(&Transform, &Realm, &mut BlockBeneathFeet), With<M>>,
) {
    let Some(world) = world else { return };

    for (transform, realm, mut block_beneath) in query.iter_mut() {
        block_beneath.0 =
            find_block_beneath_feet(&*world, transform.translation, *realm, PLAYER_QUERY_BOUNDS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ground_normal_threshold() {
        // 0.7 should be approximately 45 degrees
        let angle = MIN_GROUND_NORMAL_Y.acos().to_degrees();
        assert!(angle > 40.0 && angle < 50.0, "Threshold angle: {}", angle);
    }

    /// Mock world for testing
    struct TestWorld;

    impl BlockAccess for TestWorld {
        fn get_block_safe(&self, pos: BlockPos) -> Block {
            // Floor starts below y=0
            if pos.y < 0 {
                Block::Granite
            } else {
                Block::Air
            }
        }

        fn is_chunk_loaded(&self, _chunk_pos: crate::world::pos::pos3d::ChunkPos) -> bool {
            true
        }
    }

    #[test]
    fn test_block_beneath_feet_query() {
        let world = TestWorld;

        // At y=0.5 we should see the granite floor below (feet at ~-0.35)
        let block = find_block_beneath_feet(
            &world,
            Vec3::new(0.0, 0.5, 0.0),
            Realm::Overworld,
            PLAYER_QUERY_BOUNDS,
        );
        assert_eq!(block, Block::Granite);

        // Far above the floor should return air
        let block = find_block_beneath_feet(
            &world,
            Vec3::new(0.0, 5.0, 0.0),
            Realm::Overworld,
            PLAYER_QUERY_BOUNDS,
        );
        assert_eq!(block, Block::Air);
    }
}
