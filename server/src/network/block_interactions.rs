use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use shared::net::lightyear_protocol::{
    BlockChangeConfirm, BlockInteractionChannel, BlockInteractionRequest, CharacterMarker,
};
use shared::physics::Position;
use shared::world::BlockAccess;

use crate::world::voxel_world::VoxelWorld;

const MAX_INTERACTION_DISTANCE: f32 = 10.0;

pub struct BlockInteractionsPlugin;

impl Plugin for BlockInteractionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_block_interactions);
    }
}

fn handle_block_interactions(
    mut client_query: Query<
        (
            Entity,
            &RemoteId,
            &mut MessageReceiver<BlockInteractionRequest>,
        ),
        With<ClientOf>,
    >,
    world: Option<Res<VoxelWorld>>,
    // Query characters to find the player's position
    character_query: Query<(&Position, &ControlledBy), With<CharacterMarker>>,
    // For broadcasting to all clients
    mut sender: ServerMultiMessageSender,
    server: Query<&Server>,
) {
    let Some(world) = world else {
        return;
    };

    let Ok(server) = server.single() else {
        return;
    };

    for (client_entity, remote_id, mut receiver) in &mut client_query {
        let player_position = character_query
            .iter()
            .find(|(_, controlled_by)| controlled_by.owner == client_entity)
            .map(|(pos, _)| pos.0);

        for request in receiver.receive() {
            let block_position = request.position;
            let new_block = request.new_block;

            let Some(player_pos) = player_position else {
                warn!(
                    "Block interaction from client {:?} with no character entity",
                    remote_id
                );
                continue;
            };

            let block_center = Vec3::new(
                block_position.x as f32 + 0.5,
                block_position.y as f32 + 0.5,
                block_position.z as f32 + 0.5,
            );
            let distance = player_pos.distance(block_center);

            if distance > MAX_INTERACTION_DISTANCE {
                warn!(
                    "Client {:?} tried to interact with block at {:?} from distance {:.1} (max: {})",
                    remote_id, block_position, distance, MAX_INTERACTION_DISTANCE
                );
                continue;
            }

            let old_block = world.get_block_safe(block_position);

            if world.set_block_safe(block_position, new_block) {
                debug!(
                    "Client {:?} set block at {:?} to {:?}",
                    remote_id, block_position, new_block
                );

                let confirm = BlockChangeConfirm {
                    position: block_position,
                    old_block,
                    new_block,
                };

                if let Err(e) =
                    sender.send::<_, BlockInteractionChannel>(&confirm, server, &NetworkTarget::All)
                {
                    error!("Failed to broadcast block change: {:?}", e);
                }
            } else {
                warn!(
                    "Client {:?} failed to set block at {:?} (out of bounds or invalid)",
                    remote_id, block_position
                );
            }
        }
    }
}
