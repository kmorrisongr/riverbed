//! Shared avian3d physics plugin wiring used by both client and server.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::PLAYER_GRAVITY;

/// Plugin that configures the shared physics world (gravity, collision detection).
///
/// This plugin sets up avian3d's physics system with settings appropriate for
/// a voxel game. Both client and server should add this plugin to ensure
/// identical physics simulation.
#[derive(Default)]
pub struct SharedPhysicsWorldPlugin;

impl Plugin for SharedPhysicsWorldPlugin {
    fn build(&self, app: &mut App) {
        // Use full physics plugins for collision detection on both client and server
        app.add_plugins(PhysicsPlugins::default().with_length_unit(1.0));

        // Configure gravity for the world
        app.insert_resource(Gravity(Vec3::new(0.0, -PLAYER_GRAVITY, 0.0)));
    }
}
