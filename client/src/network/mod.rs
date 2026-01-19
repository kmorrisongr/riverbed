pub mod buffered_client;
pub mod lightyear_client;
pub mod models;
pub mod reconciliation;
mod setup;

pub use lightyear_client::LightyearClientPlugin;
pub use reconciliation::ServerAuthorityReconciliationPlugin;
pub use setup::*;

use bevy::prelude::*;

use crate::network::buffered_client::SyncTime;

pub struct NetworkPlugin;
impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        // Initialize resources needed by lightyear client
        app.init_resource::<CurrentPlayerProfile>()
            .init_resource::<SyncTime>()
            .init_resource::<SelectedWorld>()
            .init_resource::<ServerTickAtConnect>()
            .init_resource::<WorldSeed>();

        // Startup systems - launch local server if needed
        app.add_systems(Startup, launch_local_server_system);

        // Add the lightyear client plugin (handles all networking)
        app.add_plugins(LightyearClientPlugin);
    }
}
