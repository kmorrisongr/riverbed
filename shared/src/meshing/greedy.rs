//! Greedy meshing for voxel chunk data.
//!
//! This module extracts quads from chunk voxel data using the binary greedy meshing
//! algorithm. The extracted quads can then be converted to either render meshes
//! (on the client) or collision meshes (for avian3d physics).

use std::collections::BTreeSet;

use binary_greedy_meshing as bgm;

use crate::block::{Block, Face};
use crate::world::chunk::Chunk;
use crate::world::pos::{linearize, pad_linearize};
use crate::world::{CHUNKP_S3, CHUNK_S1};

/// Data for a single quad extracted from greedy meshing.
///
/// Layout optimized for cache efficiency (16 bytes total):
/// - Packs position into u32 (xyz uses only 18 bits)
/// - Uses u8 for width/height (max 62)
/// - Block types last for alignment
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct QuadData {
    /// Packed xyz position: 6 bits each for x, y, z (18 bits used of 32)
    /// Also encodes width/height in upper bits from bgm format
    pub xyz: u32,
    /// Width of the quad (1-62)
    pub width: u8,
    /// Height of the quad (1-62)
    pub height: u8,
    /// The block type
    pub block: Block,
    /// The neighbor block (block in front of this face)
    pub neighbor_block: Block,
}

impl QuadData {
    /// Extract X coordinate (0-63)
    #[inline]
    pub fn x(&self) -> u8 {
        (self.xyz & 0x3F) as u8
    }

    /// Extract Y coordinate (0-63)
    #[inline]
    pub fn y(&self) -> u8 {
        ((self.xyz >> 6) & 0x3F) as u8
    }

    /// Extract Z coordinate (0-63)
    #[inline]
    pub fn z(&self) -> u8 {
        ((self.xyz >> 12) & 0x3F) as u8
    }
}

/// Quads extracted from a chunk, organized by face direction.
pub struct ChunkQuads {
    /// Quads for each of the 6 faces (Left, Down, Back, Right, Up, Front)
    pub faces: [Vec<QuadData>; 6],
}

impl ChunkQuads {
    /// Check if all faces have no quads (empty chunk)
    pub fn is_empty(&self) -> bool {
        self.faces.iter().all(|f| f.is_empty())
    }

    /// Total number of quads across all faces
    pub fn total_quads(&self) -> usize {
        self.faces.iter().map(|f| f.len()).sum()
    }
}

/// Extract quads from a chunk using greedy meshing.
///
/// This performs the binary greedy meshing algorithm on the chunk data,
/// producing a set of quads for each face direction. These quads can then
/// be converted to render meshes or collision meshes.
///
/// # Arguments
/// * `chunk` - The chunk to mesh
/// * `lod` - Level of detail (1 = full detail, 2 = half, etc.)
///
/// # Returns
/// A `ChunkQuads` containing the extracted quads organized by face.
pub fn extract_quads(chunk: &Chunk, lod: usize) -> ChunkQuads {
    let voxels = voxel_data_lod(chunk, lod);
    let palette = chunk.palette.clone();

    let mut mesher: bgm::Mesher<CHUNK_S1> = bgm::Mesher::new();

    // Build set of transparent block indices for the mesher
    let transparents = BTreeSet::from_iter(palette.iter().enumerate().filter_map(|(i, block)| {
        if i != 0 && !block.is_opaque() {
            Some(i as u16)
        } else {
            None
        }
    }));

    mesher.mesh(&voxels, &transparents);

    let mut faces: [Vec<QuadData>; 6] = core::array::from_fn(|_| Vec::new());

    for (face_n, quads) in mesher.quads.iter().enumerate() {
        let face: Face = face_n.into();
        let offset = face.quad_to_block();

        for quad in quads {
            let voxel_i = quad.voxel_id() as usize;
            let [x, y, z] = quad.xyz();

            let block = palette[voxel_i];
            let neighbor_block = palette[voxels[linearize(
                (offset[0] + x as i32 + 1) as usize,
                (offset[1] + y as i32 + 1) as usize,
                (offset[2] + z as i32 + 1) as usize,
            )] as usize];

            faces[face_n].push(QuadData {
                xyz: (quad.0 & MASK_XYZ) as u32,
                width: quad.width() as u8,
                height: quad.height() as u8,
                block,
                neighbor_block,
            });
        }
    }

    ChunkQuads { faces }
}

/// Mask to extract XYZ from packed quad data
const MASK_XYZ: u64 = 0b111111_111111_111111;

/// Convert chunk data to voxel array with LOD support.
///
/// For LOD=1, returns the unpacked data directly.
/// For LOD>1, reads directly from the packed data during downsampling
/// to avoid allocating an intermediate full-resolution array.
fn voxel_data_lod(chunk: &Chunk, lod: usize) -> Vec<u16> {
    if lod == 1 {
        // Fast path: just unpack directly
        return chunk.data.unpack_u16();
    }

    // For LOD > 1: read directly from packed data during downsampling
    // This avoids allocating a full CHUNKP_S3 array just to downsample it
    let mut res = vec![0u16; CHUNKP_S3];
    for x in 0..CHUNK_S1 {
        for y in 0..CHUNK_S1 {
            for z in 0..CHUNK_S1 {
                let lod_i = pad_linearize(x / lod, y / lod, z / lod);
                if res[lod_i] == 0 {
                    // Read directly from packed storage instead of unpacking first
                    res[lod_i] = chunk.data.get(pad_linearize(x, y, z)) as u16;
                }
            }
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_quads_empty_chunk() {
        let chunk = Chunk::new();
        let quads = extract_quads(&chunk, 1);
        assert!(quads.is_empty());
    }

    #[test]
    fn test_extract_quads_single_block() {
        let mut chunk = Chunk::new();
        chunk.set((1, 1, 1), Block::Granite);
        let quads = extract_quads(&chunk, 1);
        // A single block should produce 6 quads (one per face)
        assert_eq!(quads.total_quads(), 6);
    }
}
