use avian3d::prelude::Collisions;
use bevy::prelude::*;

use crate::block::Block;
use crate::physics::{PLAYER_CAPSULE_HEIGHT, PLAYER_CAPSULE_RADIUS, PLAYER_QUERY_BOUNDS};
use crate::world::block_access::BlockAccess;
use crate::world::pos::pos3d::BlockPos;
use crate::world::realm::Realm;

pub const MIN_GROUND_NORMAL_Y_THRESHOLD: f32 = 0.7;

#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Grounded(pub bool);

#[derive(Component, Debug, Clone, Copy)]
pub struct BlockBeneathFeet(pub Block);

impl Default for BlockBeneathFeet {
    fn default() -> Self {
        Self(Block::Air)
    }
}

fn footprint_block_positions(
    capsule_center: Vec3,
    realm: Realm,
    footprint_size: Vec3,
) -> impl Iterator<Item = BlockPos> {
    let feet_y = capsule_center.y - (PLAYER_CAPSULE_HEIGHT / 2.0 + PLAYER_CAPSULE_RADIUS);
    let y = (feet_y - 0.01).floor() as i32;

    let x_start = (capsule_center.x - footprint_size.x / 2.0).floor() as i32;
    let x_end = (capsule_center.x + footprint_size.x / 2.0).floor() as i32;
    let z_start = (capsule_center.z - footprint_size.z / 2.0).floor() as i32;
    let z_end = (capsule_center.z + footprint_size.z / 2.0).floor() as i32;

    (x_start..=x_end)
        .flat_map(move |x| (z_start..=z_end).map(move |z| (x, z)))
        .map(move |(x, z)| BlockPos { x, y, z, realm })
}

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

        let dist_sq = (capsule_center.x - (block_pos.x as f32 + 0.5)).powi(2)
            + (capsule_center.z - (block_pos.z as f32 + 0.5)).powi(2);

        if dist_sq < min_dist_sq {
            min_dist_sq = dist_sq;
            closest_block = block;
        }
    }
    closest_block
}

pub fn check_grounded_from_collisions(collisions: &Collisions, entity: Entity) -> bool {
    for contact_pair in collisions.collisions_with(entity) {
        if !contact_pair.is_touching() {
            continue;
        }

        let is_first = contact_pair.body1 == Some(entity) || contact_pair.collider1 == entity;

        for manifold in &contact_pair.manifolds {
            let effective_y = if is_first {
                -manifold.normal.y
            } else {
                manifold.normal.y
            };

            if effective_y > MIN_GROUND_NORMAL_Y_THRESHOLD {
                return true;
            }
        }
    }
    false
}

pub fn sync_grounded_state<M: Component>(
    collisions: Collisions,
    mut query: Query<(Entity, &mut Grounded), With<M>>,
) {
    for (entity, mut grounded) in query.iter_mut() {
        grounded.0 = check_grounded_from_collisions(&collisions, entity);
    }
}

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
        let angle = MIN_GROUND_NORMAL_Y_THRESHOLD.acos().to_degrees();
        assert!(angle > 40.0 && angle < 50.0, "Threshold angle: {}", angle);
    }

    struct TestWorld;

    impl BlockAccess for TestWorld {
        fn get_block_safe(&self, pos: BlockPos) -> Block {
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
