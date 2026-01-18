//! Ground detection using avian3d collision contacts.
//!
//! This module provides systems to update ground state based on avian3d's
//! collision detection. A contact with an upward-pointing normal indicates
//! the player is standing on something.

use avian3d::prelude::Collisions;
use bevy::prelude::*;

/// Minimum Y component of contact normal to count as "ground".
/// 0.7 corresponds to approximately a 45-degree slope.
pub const GROUND_NORMAL_THRESHOLD: f32 = 0.7;

/// Component that tracks whether an entity is on the ground.
/// Updated each frame from avian3d collision data.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct OnGround(pub bool);

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
