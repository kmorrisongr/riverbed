#![allow(dead_code)]
//! Minimal scaffolding for a future Lightyear server pipeline. Feature-gated
//! behind `lightyear-net` so it can be iterated without impacting Renet.

use bevy::prelude::*;
use shared::messages::ActionMask;
use shared::net::lightyear_protocol::LightyearProtocolPlugin;

/// Latest inputs received from clients (placeholder until Lightyear transport is wired).
#[derive(Resource, Default, Debug, Clone)]
pub struct LightyearServerInbox {
    pub pending: Vec<(PeerId, u64, ActionMask)>,
}

pub struct LightyearServerPlugin;

impl Plugin for LightyearServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(LightyearProtocolPlugin)
            .init_resource::<LightyearServerInbox>();
    }
}

/// Stub PeerId so the scaffolding compiles before Lightyear transport is added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PeerId(pub u64);
