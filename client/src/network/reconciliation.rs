//! Server authority reconciliation.
//!
//! With lightyear, reconciliation is handled automatically by the prediction
//! rollback system. This module is kept as a placeholder for any additional
//! reconciliation logic that may be needed in the future.

use bevy::prelude::*;

pub struct ServerAuthorityReconciliationPlugin;

impl Plugin for ServerAuthorityReconciliationPlugin {
    fn build(&self, _app: &mut App) {
        // Lightyear handles prediction rollback automatically.
        // Additional reconciliation systems can be added here if needed.
    }
}

