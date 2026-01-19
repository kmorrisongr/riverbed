pub mod lightyear_client;
mod setup;

pub use lightyear_client::LightyearClientPlugin;
pub use setup::*;

use bevy::prelude::*;

pub struct NetworkPlugin;
impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        // Initialize resources needed by lightyear client
        app.init_resource::<CurrentPlayerProfile>()
            .init_resource::<SelectedWorld>();

        // Startup systems - launch local server if needed
        app.add_systems(Startup, launch_local_server_system);

        // Add the lightyear client plugin (handles all networking)
        app.add_plugins(LightyearClientPlugin);
    }
}
