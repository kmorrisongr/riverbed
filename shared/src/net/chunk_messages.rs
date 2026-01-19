//! Chunk streaming messages sent via renet.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::world::chunk::Chunk;
use crate::world::pos::pos3d::ChunkPos;

/// World update message sent from server to clients containing new chunk data.
#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct ChunkDataMessage {
    pub tick: u64,
    pub chunks: HashMap<ChunkPos, Chunk>,
}
