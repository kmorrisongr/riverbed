//! Shared meshing utilities for voxel chunks.
//!
//! This module provides greedy meshing for voxel data that can be used by both:
//! - Client: for rendering meshes
//! - Server/Client: for physics collision meshes (avian3d trimesh colliders)
//!
//! The meshing is split into two phases:
//! 1. Quad extraction (greedy meshing) - produces a list of quads per face
//! 2. Mesh generation - converts quads to either render meshes or collision meshes

pub mod chunk_collider;
mod greedy;

pub use greedy::{extract_quads, ChunkQuads, QuadData};
