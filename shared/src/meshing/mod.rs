//! Shared meshing utilities for voxel chunks.
//!
//! This module provides greedy meshing for voxel data that can be used by both:
//! - **Client**: for rendering meshes (visual representation)
//! - **Client/Server**: for physics collision meshes (avian3d trimesh colliders)
//!
//! # Meshing Pipeline
//!
//! The meshing is split into two phases:
//! 1. **Quad extraction** (`extract_quads`): Greedy meshing produces a list of quads per face
//! 2. **Mesh generation**: Converts quads to either render meshes or collision meshes
//!
//! # Physics Integration
//!
//! For physics, use `ChunkColliderPlugin<P>` where `P: ChunkProvider`. This plugin:
//! - Listens for `ChunkColliderRebuildRequest` events
//! - Generates `StaticChunkColliderBundle` entities with trimesh colliders
//! - Tracks collider entities in `ChunkColliderEntityRegistry`
//! - Automatically despawns colliders when columns are unloaded

pub mod chunk_collider;
pub mod collider_sync;
mod greedy;

pub use collider_sync::{
    ChunkColliderEntityRegistry, ChunkColliderPlugin, ChunkColliderRebuildRequest, ChunkProvider,
};
pub use greedy::{extract_quads, ChunkQuads, QuadData};
