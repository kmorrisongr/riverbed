//! Chunk collider generation for avian3d physics.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::block::Face;
use crate::meshing::{extract_quads, ChunkQuads};
use crate::physics::collision_layers_for_realm;
use crate::world::chunk::Chunk;
use crate::world::pos::pos3d::ChunkPos;
use crate::world::CHUNK_S1;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticChunkColliderTag {
    pub chunk_pos: ChunkPos,
}

#[derive(Bundle)]
pub struct StaticChunkColliderBundle {
    pub tag: StaticChunkColliderTag,
    pub collider: Collider,
    pub rigid_body: RigidBody,
    pub position: Position,
    pub rotation: Rotation,
    pub collision_layers: CollisionLayers,
}

impl StaticChunkColliderBundle {
    pub fn new(chunk: &Chunk, chunk_pos: ChunkPos, lod: usize) -> Option<Self> {
        let collider = generate_chunk_trimesh_collider(chunk, lod)?;
        Some(Self::from_collider(collider, chunk_pos))
    }

    pub fn from_collider(collider: Collider, chunk_pos: ChunkPos) -> Self {
        let world_pos = Vec3::new(
            (chunk_pos.x * CHUNK_S1 as i32) as f32,
            (chunk_pos.y * CHUNK_S1 as i32) as f32,
            (chunk_pos.z * CHUNK_S1 as i32) as f32,
        );

        Self {
            tag: StaticChunkColliderTag { chunk_pos },
            collider,
            rigid_body: RigidBody::Static,
            position: Position(world_pos),
            rotation: Rotation::default(),
            collision_layers: collision_layers_for_realm(chunk_pos.realm),
        }
    }
}

pub fn generate_chunk_trimesh_collider(chunk: &Chunk, lod: usize) -> Option<Collider> {
    let quads = extract_quads(chunk, lod);

    if quads.is_empty() {
        return None;
    }

    let (vertices, indices) = quads_to_trimesh(&quads);

    if vertices.is_empty() || indices.is_empty() {
        return None;
    }

    Some(Collider::trimesh(vertices, indices))
}

fn quads_to_trimesh(quads: &ChunkQuads) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let total_quads = quads.total_quads();
    let mut vertices = Vec::with_capacity(total_quads * 4);
    let mut indices = Vec::with_capacity(total_quads * 2);

    for (face_n, face_quads) in quads.faces.iter().enumerate() {
        let face: Face = face_n.into();

        for quad in face_quads {
            if quad.block.is_traversable() {
                continue;
            }
            let base_vertex = vertices.len() as u32;

            let quad_verts = face.quad_vertices(
                quad.x() as f32,
                quad.y() as f32,
                quad.z() as f32,
                quad.width as f32,
                quad.height as f32,
            );

            vertices.extend(quad_verts.map(|v| Vec3::from_array(v)));

            let (tri1, tri2) = face.triangle_indices(base_vertex);
            indices.push(tri1);
            indices.push(tri2);
        }
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::realm::Realm;

    #[test]
    fn test_empty_chunk_no_collider() {
        let chunk = Chunk::new();
        let collider = generate_chunk_trimesh_collider(&chunk, 1);
        assert!(collider.is_none());
    }

    #[test]
    fn test_single_block_generates_collider() {
        use crate::block::Block;

        let mut chunk = Chunk::new();
        chunk.set((1, 1, 1), Block::Granite);

        let collider = generate_chunk_trimesh_collider(&chunk, 1);
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

        let bundle = StaticChunkColliderBundle::new(&chunk, chunk_pos, 1);
        assert!(bundle.is_some());
    }
}
