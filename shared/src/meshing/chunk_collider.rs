//! Chunk collider generation for avian3d physics.
//!
//! This module generates trimesh colliders from chunk geometry for use with
//! avian3d's physics system. Instead of having one collider per block, we
//! generate a single trimesh collider per chunk from the greedy-meshed surface.
//!
//! This approach:
//! - Reduces collider count dramatically (one per chunk vs thousands per block)
//! - Leverages avian3d's broadphase acceleration
//! - Handles edges and corners correctly
//! - Only rebuilds when chunks change

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::block::Face;
use crate::meshing::{extract_quads, ChunkQuads};
use crate::world::chunk::Chunk;
use crate::world::pos::pos3d::ChunkPos;
use crate::world::CHUNK_S1;

/// Component to identify chunk collider entities
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkCollider {
    pub chunk_pos: ChunkPos,
}

/// Bundle for spawning a chunk collider entity
#[derive(Bundle)]
pub struct ChunkColliderBundle {
    pub chunk_collider: ChunkCollider,
    pub collider: Collider,
    pub rigid_body: RigidBody,
    pub position: Position,
    pub rotation: Rotation,
}

impl ChunkColliderBundle {
    /// Create a new chunk collider bundle from a chunk.
    ///
    /// Returns `None` if the chunk has no solid geometry (all air).
    pub fn new(chunk: &Chunk, chunk_pos: ChunkPos) -> Option<Self> {
        let collider = generate_chunk_collider(chunk)?;

        // Calculate world position of chunk origin
        let world_pos = Vec3::new(
            (chunk_pos.x * CHUNK_S1 as i32) as f32,
            (chunk_pos.y * CHUNK_S1 as i32) as f32,
            (chunk_pos.z * CHUNK_S1 as i32) as f32,
        );

        Some(Self {
            chunk_collider: ChunkCollider { chunk_pos },
            collider,
            rigid_body: RigidBody::Static,
            position: Position(world_pos.into()),
            rotation: Rotation::default(),
        })
    }
}

/// Generate a trimesh collider from chunk geometry.
///
/// This extracts the surface quads from the chunk using greedy meshing,
/// then converts them to triangles for the physics collider.
///
/// Returns `None` if the chunk has no solid geometry.
pub fn generate_chunk_collider(chunk: &Chunk) -> Option<Collider> {
    let quads = extract_quads(chunk, 1);

    if quads.is_empty() {
        return None;
    }

    let (vertices, indices) = quads_to_trimesh(&quads);

    if vertices.is_empty() || indices.is_empty() {
        return None;
    }

    // Create trimesh collider from vertices and triangle indices
    Some(Collider::trimesh(vertices, indices))
}

/// Convert extracted quads to trimesh vertices and indices.
///
/// Each quad becomes two triangles (6 indices).
fn quads_to_trimesh(quads: &ChunkQuads) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let total_quads = quads.total_quads();
    let mut vertices = Vec::with_capacity(total_quads * 4);
    let mut indices = Vec::with_capacity(total_quads * 2);

    for (face_n, face_quads) in quads.faces.iter().enumerate() {
        let face: Face = face_n.into();

        for quad in face_quads {
            let base_vertex = vertices.len() as u32;

            // Get the 4 corners of the quad in world space
            let quad_vertices = face_quad_vertices(
                face,
                quad.x() as f32,
                quad.y() as f32,
                quad.z() as f32,
                quad.width as f32,
                quad.height as f32,
            );

            vertices.extend_from_slice(&quad_vertices);

            // Two triangles per quad (winding order for correct normals)
            // The winding order depends on the face direction
            let (tri1, tri2) = face_triangle_indices(face, base_vertex);
            indices.push(tri1);
            indices.push(tri2);
        }
    }

    (vertices, indices)
}

/// Get the 4 corner vertices of a quad for a given face.
///
/// The coordinates are in chunk-local space (0 to CHUNK_S1).
fn face_quad_vertices(face: Face, x: f32, y: f32, z: f32, w: f32, h: f32) -> [Vec3; 4] {
    match face {
        Face::Left => [
            Vec3::new(x, y, z),
            Vec3::new(x, y, z + h),
            Vec3::new(x, y + w, z),
            Vec3::new(x, y + w, z + h),
        ],
        Face::Right => [
            Vec3::new(x + 1.0, y, z),
            Vec3::new(x + 1.0, y + w, z),
            Vec3::new(x + 1.0, y, z + h),
            Vec3::new(x + 1.0, y + w, z + h),
        ],
        Face::Down => [
            Vec3::new(x, y, z),
            Vec3::new(x + w, y, z),
            Vec3::new(x, y, z + h),
            Vec3::new(x + w, y, z + h),
        ],
        Face::Up => [
            Vec3::new(x, y + 1.0, z),
            Vec3::new(x, y + 1.0, z + h),
            Vec3::new(x + w, y + 1.0, z),
            Vec3::new(x + w, y + 1.0, z + h),
        ],
        Face::Back => [
            Vec3::new(x, y, z),
            Vec3::new(x, y + h, z),
            Vec3::new(x + w, y, z),
            Vec3::new(x + w, y + h, z),
        ],
        Face::Front => [
            Vec3::new(x, y, z + 1.0),
            Vec3::new(x + w, y, z + 1.0),
            Vec3::new(x, y + h, z + 1.0),
            Vec3::new(x + w, y + h, z + 1.0),
        ],
    }
}

/// Get triangle indices for a quad with correct winding order.
///
/// The winding order is set so that the normal points outward from the solid block.
fn face_triangle_indices(face: Face, base: u32) -> ([u32; 3], [u32; 3]) {
    match face {
        // Outward-facing normals (counter-clockwise when viewed from outside)
        Face::Right | Face::Up | Face::Front => (
            [base, base + 1, base + 2],
            [base + 2, base + 1, base + 3],
        ),
        // Inward-facing normals need opposite winding
        Face::Left | Face::Down | Face::Back => (
            [base, base + 2, base + 1],
            [base + 2, base + 3, base + 1],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::realm::Realm;

    #[test]
    fn test_empty_chunk_no_collider() {
        let chunk = Chunk::new();
        let collider = generate_chunk_collider(&chunk);
        assert!(collider.is_none());
    }

    #[test]
    fn test_single_block_generates_collider() {
        use crate::block::Block;

        let mut chunk = Chunk::new();
        chunk.set((1, 1, 1), Block::Granite);

        let collider = generate_chunk_collider(&chunk);
        assert!(collider.is_some());
    }

    #[test]
    fn test_chunk_collider_bundle() {
        use crate::block::Block;

        let mut chunk = Chunk::new();
        chunk.set((1, 1, 1), Block::Granite);

        let chunk_pos = ChunkPos {
            x: 0,
            y: 0,
            z: 0,
            realm: Realm::Overworld,
        };

        let bundle = ChunkColliderBundle::new(&chunk, chunk_pos);
        assert!(bundle.is_some());
    }
}
