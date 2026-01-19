//! Renet configuration for chunk streaming.
//!
//! This module provides the channel configuration and codec functions needed for
//! streaming chunk data between server and clients using renet. This coexists with
//! the lightyear-based networking for player replication.

use std::time::Duration;

use bevy::log::debug;
use bevy_renet::renet::{ChannelConfig, ConnectionConfig, SendType};
use bincode::Options;

use crate::utils::format_bytes;

const MAX_MEMORY: usize = 128 * 1024 * 1024;
const RESEND_TIME: Duration = Duration::from_millis(300);
const AVAILABLE_BYTES_PER_TICK: u64 = 5 * 1024 * 1024;

/// Server-to-client channel for chunk data
pub const STC_CHUNK_DATA_CHANNEL: u8 = 0;

pub fn get_chunk_server_to_client_channels() -> Vec<ChannelConfig> {
    vec![ChannelConfig {
        channel_id: STC_CHUNK_DATA_CHANNEL,
        max_memory_usage_bytes: MAX_MEMORY,
        send_type: SendType::ReliableOrdered {
            resend_time: RESEND_TIME,
        },
    }]
}

pub fn get_chunk_client_to_server_channels() -> Vec<ChannelConfig> {
    // Client doesn't send chunk data, but renet requires matching channels
    vec![]
}

pub fn get_chunk_renet_config() -> ConnectionConfig {
    ConnectionConfig {
        client_channels_config: get_chunk_client_to_server_channels(),
        server_channels_config: get_chunk_server_to_client_channels(),
        available_bytes_per_tick: AVAILABLE_BYTES_PER_TICK,
    }
}

/// Compress and serialize a message for transmission
pub fn chunk_message_to_payload<T: serde::Serialize>(message: &T) -> Vec<u8> {
    let payload = bincode::options().serialize(message).unwrap();
    let output = lz4::block::compress(&payload, None, true).unwrap();
    if payload.len() > 1024 {
        debug!(
            "Original payload size: {}",
            format_bytes(payload.len() as u64)
        );
        debug!(
            "Compressed payload of size: {}",
            format_bytes(output.len() as u64)
        );
    }
    output
}

/// Decompress and deserialize a received payload
pub fn payload_to_chunk_message<T: serde::de::DeserializeOwned>(
    payload: &[u8],
) -> Result<T, bincode::Error> {
    let decompressed_payload = lz4::block::decompress(payload, None)?;
    bincode::options().deserialize(&decompressed_payload)
}
