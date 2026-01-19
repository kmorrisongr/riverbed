//! Time synchronization utilities for the client.

use bevy::prelude::*;
use shared::net::clock::TickClock;

/// Synchronized time tracking for the client.
/// Used for input timestamping and server time synchronization.
#[derive(Resource)]
pub struct SyncTime {
    pub clock: TickClock,
}

impl Default for SyncTime {
    fn default() -> Self {
        Self {
            clock: TickClock::new(),
        }
    }
}

pub trait SyncTimeExt {
    fn delta(&self) -> u64;
    fn advance(&mut self);
    fn set_offset(&mut self, offset_ms: i64);
    fn now_synced(&self) -> i64;
}

impl SyncTimeExt for SyncTime {
    fn delta(&self) -> u64 {
        self.clock.delta()
    }

    fn advance(&mut self) {
        self.clock.advance();
    }

    fn set_offset(&mut self, offset_ms: i64) {
        self.clock.set_offset(offset_ms);
    }

    fn now_synced(&self) -> i64 {
        self.clock.synced_now_ms()
    }
}
