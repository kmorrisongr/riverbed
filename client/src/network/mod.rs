pub mod chunk_streaming;
pub mod lightyear_client;
mod setup;

pub use chunk_streaming::ChunkStreamingClientPlugin;
pub use lightyear_client::LightyearClientPlugin;
pub use setup::*;

use bevy::prelude::*;

pub struct NetworkPlugin;
impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentPlayerProfile>()
            .init_resource::<SelectedWorld>();

        app.add_systems(Startup, launch_local_server_system);

        app.add_plugins(LightyearClientPlugin);
        app.add_plugins(ChunkStreamingClientPlugin);
    }
}
