use binary_greedy_meshing as bgm;

use crate::block::{Block, Face};
use crate::world::chunk::Chunk;
use crate::world::pos::{linearize, pad_linearize};
use crate::world::{CHUNKP_S3, CHUNK_S1};

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct QuadData {
    /// Packed xyz position: 6 bits each for x, y, z (18 bits used of 32)
    /// Also encodes width/height in upper bits from bgm format
    pub packed_xyz: u32,
    pub width: u8,
    pub height: u8,
    pub block: Block,
    pub neighbor_block: Block,
}

impl QuadData {
    #[inline]
    pub fn x(&self) -> u8 {
        (self.packed_xyz & 0x3F) as u8
    }

    #[inline]
    pub fn y(&self) -> u8 {
        ((self.packed_xyz >> 6) & 0x3F) as u8
    }

    #[inline]
    pub fn z(&self) -> u8 {
        ((self.packed_xyz >> 12) & 0x3F) as u8
    }
}

pub struct ChunkQuads {
    pub faces: [Vec<QuadData>; 6],
}

impl ChunkQuads {
    pub fn is_empty(&self) -> bool {
        self.faces.iter().all(|f| f.is_empty())
    }

    pub fn total_quads(&self) -> usize {
        self.faces.iter().map(|f| f.len()).sum()
    }
}

pub fn extract_quads(chunk: &Chunk, lod: usize) -> ChunkQuads {
    let voxels = voxel_data_lod(chunk, lod);
    let palette = chunk.palette.clone();

    // Precompute transparency flags once per chunk
    let mut is_transparent = vec![false; palette.iter().len()];
    for (i, block) in palette.iter().enumerate() {
        if i != 0 && !block.is_opaque() {
            is_transparent[i] = true;
        }
    }

    let mut mesher: bgm::Mesher<CHUNK_S1> = bgm::Mesher::new();

    // Build bit masks for fast meshing (avoids BTreeSet lookups)
    let mut opaque_mask = vec![0u64; bgm::Mesher::<CHUNK_S1>::CS_P2];
    let mut trans_mask = vec![0u64; bgm::Mesher::<CHUNK_S1>::CS_P2];

    for (i, voxel) in voxels.iter().enumerate() {
        if *voxel == 0 {
            continue;
        }
        let (col, bit) = (
            i / bgm::Mesher::<CHUNK_S1>::CS_P,
            i % bgm::Mesher::<CHUNK_S1>::CS_P,
        );
        if is_transparent[*voxel as usize] {
            trans_mask[col] |= 1 << bit;
        } else {
            opaque_mask[col] |= 1 << bit;
        }
    }

    mesher.fast_mesh(&voxels, &opaque_mask, &trans_mask);

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
                packed_xyz: (quad.0 & MASK_XYZ) as u32,
                width: quad.width() as u8,
                height: quad.height() as u8,
                block,
                neighbor_block,
            });
        }
    }

    ChunkQuads { faces }
}

const MASK_XYZ: u64 = 0b111111_111111_111111;

fn voxel_data_lod(chunk: &Chunk, lod: usize) -> Vec<u16> {
    if lod == 1 {
        return chunk.data.unpack_u16();
    }

    let mut lod_voxels = vec![0u16; CHUNKP_S3];
    let step = lod;

    for base_x in (0..CHUNK_S1).step_by(step) {
        let max_x = (base_x + step).min(CHUNK_S1);
        for base_y in (0..CHUNK_S1).step_by(step) {
            let max_y = (base_y + step).min(CHUNK_S1);
            for base_z in (0..CHUNK_S1).step_by(step) {
                let max_z = (base_z + step).min(CHUNK_S1);
                let lod_index = pad_linearize(base_x / step, base_y / step, base_z / step);

                if lod_voxels[lod_index] != 0 {
                    continue;
                }

                'cell: for x in base_x..max_x {
                    for y in base_y..max_y {
                        for z in base_z..max_z {
                            let voxel = chunk.data.get(pad_linearize(x, y, z)) as u16;
                            if voxel != 0 {
                                lod_voxels[lod_index] = voxel;
                                break 'cell;
                            }
                        }
                    }
                }
            }
        }
    }

    lod_voxels
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
