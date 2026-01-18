//! Ground detection using avian3d collision contacts.
//!
//! This module provides components and systems for ground state tracking:
//! - `OnGround`: Whether an entity is standing on a surface (from avian3d contacts)
//! - `SteppingOn`: Which block type the entity is standing on (for surface effects)

use avian3d::prelude::Collisions;
use bevy::prelude::*;

use crate::block::Block;

/// Minimum Y component of contact normal to count as "ground".
/// 0.7 corresponds to approximately a 45-degree slope.
pub const GROUND_NORMAL_THRESHOLD: f32 = 0.7;

/// Component that tracks whether an entity is on the ground.
/// Updated each frame from avian3d collision data.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct OnGround(pub bool);

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
pub struct SteppingOn(pub Block);

impl Default for SteppingOn {
    fn default() -> Self {
        Self(Block::Air)
    }
}

/// Check if any contact normal indicates standing on ground.
///
/// A contact is considered "ground" if:
/// - The contact is currently touching (not just a sensor overlap)
/// - The contact normal has a Y component > GROUND_NORMAL_THRESHOLD
///
/// The normal points from first shape to second, so we need to check
/// both directions depending on which entity we're querying for.
pub fn is_on_ground_from_contacts(collisions: &Collisions, entity: Entity) -> bool {
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

            if effective_y > GROUND_NORMAL_THRESHOLD {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ground_normal_threshold() {
        // 0.7 should be approximately 45 degrees
        let angle = GROUND_NORMAL_THRESHOLD.acos().to_degrees();
        assert!(angle > 40.0 && angle < 50.0, "Threshold angle: {}", angle);
    }
}
