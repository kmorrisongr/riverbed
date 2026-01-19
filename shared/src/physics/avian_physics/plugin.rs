//! Shared avian3d physics plugin wiring.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::PLAYER_GRAVITY;

#[derive(Default)]
pub struct SharedPhysicsWorldPlugin;

impl Plugin for SharedPhysicsWorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugins::default().with_length_unit(1.0));
        app.insert_resource(Gravity(Vec3::new(0.0, -PLAYER_GRAVITY, 0.0)));
    }
}
