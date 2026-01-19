use bevy::ecs::component::Component;
use strum_macros::EnumIter;
const UP_SPECIFIER: [FaceSpecifier; 2] = [FaceSpecifier::Specific(Face::Up), FaceSpecifier::All];
const DOWN_SPECIFIER: [FaceSpecifier; 3] = [
    FaceSpecifier::Specific(Face::Down),
    FaceSpecifier::Specific(Face::Up),
    FaceSpecifier::All,
];
const LEFT_SPECIFIER: [FaceSpecifier; 3] = [
    FaceSpecifier::Specific(Face::Left),
    FaceSpecifier::Side,
    FaceSpecifier::All,
];
const RIGHT_SPECIFIER: [FaceSpecifier; 3] = [
    FaceSpecifier::Specific(Face::Right),
    FaceSpecifier::Side,
    FaceSpecifier::All,
];
const FRONT_SPECIFIER: [FaceSpecifier; 3] = [
    FaceSpecifier::Specific(Face::Front),
    FaceSpecifier::Side,
    FaceSpecifier::All,
];
const BACK_SPECIFIER: [FaceSpecifier; 3] = [
    FaceSpecifier::Specific(Face::Back),
    FaceSpecifier::Side,
    FaceSpecifier::All,
];

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum FaceSpecifier {
    Specific(Face),
    Side,
    All,
}

#[derive(Component, EnumIter, PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub enum Face {
    Left,
    Down,
    Back,
    Right,
    Up,
    Front,
}

impl Face {
    pub fn n(&self) -> [i32; 3] {
        match self {
            Self::Left => [-1, 0, 0],
            Self::Down => [0, -1, 0],
            Self::Back => [0, 0, -1],
            Self::Right => [1, 0, 0],
            Self::Up => [0, 1, 0],
            Self::Front => [0, 0, 1],
        }
    }

    pub fn quad_to_block(&self) -> [i32; 3] {
        match self {
            Self::Left => [-1, 0, 0],
            Self::Down => [0, -1, 0],
            Self::Back => [0, 0, -1],
            Self::Right => [0, 0, 0],
            Self::Up => [0, 0, 0],
            Self::Front => [-1, 0, 0],
        }
    }

    pub fn specifiers(&self) -> &[FaceSpecifier] {
        match self {
            Self::Left => &LEFT_SPECIFIER,
            Self::Down => &DOWN_SPECIFIER,
            Self::Back => &BACK_SPECIFIER,
            Self::Right => &RIGHT_SPECIFIER,
            Self::Up => &UP_SPECIFIER,
            Self::Front => &FRONT_SPECIFIER,
        }
    }

    pub fn opposite(&self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Down => Self::Up,
            Self::Back => Self::Front,
            Self::Right => Self::Left,
            Self::Up => Self::Down,
            Self::Front => Self::Back,
        }
    }
}

// must match RIGHT_HANDED_Y_UP_CONFIG.faces from block-mesh-rs
impl From<u8> for Face {
    fn from(value: u8) -> Self {
        assert!(value < 6);
        match value {
            0 => Self::Up,
            1 => Self::Down,
            2 => Self::Right,
            3 => Self::Left,
            4 => Self::Front,
            5 => Self::Back,
            _ => unreachable!(),
        }
    }
}

impl From<usize> for Face {
    fn from(value: usize) -> Self {
        (value as u8).into()
    }
}

//
// Note: This whole section will become unnecessary when we have instancing,
// because it is used to convert a quad to 4 vertices which we won't need to do

fn packed_xyz(x: u32, y: u32, z: u32) -> u32 {
    (z << 12) | (y << 6) | x
}

fn vertex_info(xyz: u32, u: u32, v: u32) -> u32 {
    (v << 24) | (u << 18) | xyz
}

impl Face {
    /// Get the 4 corner vertices of a quad for this face.
    ///
    /// Given a base position (x, y, z) and quad dimensions (width, height),
    /// returns the 4 corners in chunk-local space. The vertex order is
    /// consistent for use with standard triangle indices.
    ///
    /// This is the canonical source of quad geometry used by both
    /// collision mesh generation and (indirectly) rendering.
    #[inline]
    pub fn quad_vertices(&self, x: f32, y: f32, z: f32, w: f32, h: f32) -> [[f32; 3]; 4] {
        match self {
            Face::Left => [
                [x, y, z],
                [x, y, z + h],
                [x, y + w, z],
                [x, y + w, z + h],
            ],
            Face::Right => [
                [x + 1.0, y, z],
                [x + 1.0, y + w, z],
                [x + 1.0, y, z + h],
                [x + 1.0, y + w, z + h],
            ],
            Face::Down => [
                [x, y, z],
                [x + w, y, z],
                [x, y, z + h],
                [x + w, y, z + h],
            ],
            Face::Up => [
                [x, y + 1.0, z],
                [x, y + 1.0, z + h],
                [x + w, y + 1.0, z],
                [x + w, y + 1.0, z + h],
            ],
            Face::Back => [
                [x, y, z],
                [x, y + h, z],
                [x + w, y, z],
                [x + w, y + h, z],
            ],
            Face::Front => [
                [x, y, z + 1.0],
                [x + w, y, z + 1.0],
                [x, y + h, z + 1.0],
                [x + w, y + h, z + 1.0],
            ],
        }
    }

    /// Get triangle indices for a quad with correct winding order.
    ///
    /// The winding order is set so that the normal points outward from the solid block.
    /// Parry3d/Avian3d uses counter-clockwise winding for outward-facing normals.
    ///
    /// Given quad_vertices returns vertices in order [v0, v1, v2, v3], we need to
    /// select triangle indices such that the cross product (v1-v0) × (v2-v0) points
    /// in the direction of the face normal.
    ///
    /// Returns two triangles (6 indices total) as `([u32; 3], [u32; 3])`.
    #[inline]
    pub fn triangle_indices(&self, base: u32) -> ([u32; 3], [u32; 3]) {
        // The winding order must be determined per-face based on the vertex layout
        // in quad_vertices(). After analysis, the correct winding for CCW outward normals:
        match self {
            // Left face: normal should point -X
            // Vertices: [0,0,0], [0,0,h], [0,w,0], [0,w,h] (varying in Y and Z)
            // Triangle 0-1-2: edge01=(0,0,h), edge02=(0,w,0) → cross = (-wh, 0, 0) ✓ points -X
            Face::Left => {
                ([base, base + 1, base + 2], [base + 2, base + 1, base + 3])
            }
            // Right face: normal should point +X
            // Vertices: [1,0,0], [1,w,0], [1,0,h], [1,w,h] (varying in Y and Z)
            // Triangle 0-1-2: edge01=(0,w,0), edge02=(0,0,h) → cross = (wh, 0, 0) ✓ points +X
            Face::Right => {
                ([base, base + 1, base + 2], [base + 2, base + 1, base + 3])
            }
            // Down face: normal should point -Y
            // Vertices: [0,0,0], [w,0,0], [0,0,h], [w,0,h] (varying in X and Z)
            // Triangle 0-1-2: edge01=(w,0,0), edge02=(0,0,h) → cross = (0, -wh, 0) ✓ points -Y
            Face::Down => {
                ([base, base + 1, base + 2], [base + 2, base + 1, base + 3])
            }
            // Up face: normal should point +Y
            // Vertices: [0,1,0], [0,1,h], [w,1,0], [w,1,h] (varying in X and Z)
            // Triangle 0-1-2: edge01=(0,0,h), edge02=(w,0,0) → cross = (0, wh, 0) ✓ points +Y
            Face::Up => {
                ([base, base + 1, base + 2], [base + 2, base + 1, base + 3])
            }
            // Back face: normal should point -Z
            // Vertices: [0,0,0], [0,h,0], [w,0,0], [w,h,0] (varying in X and Y)
            // Triangle 0-1-2: edge01=(0,h,0), edge02=(w,0,0) → cross = (0, 0, -wh) ✓ points -Z
            Face::Back => {
                ([base, base + 1, base + 2], [base + 2, base + 1, base + 3])
            }
            // Front face: normal should point +Z
            // Vertices: [0,0,1], [w,0,1], [0,h,1], [w,h,1] (varying in X and Y)
            // Triangle 0-1-2: edge01=(w,0,0), edge02=(0,h,0) → cross = (0, 0, wh) ✓ points +Z
            Face::Front => {
                ([base, base + 1, base + 2], [base + 2, base + 1, base + 3])
            }
        }
    }
}

impl Face {
    // Note: This method will become unnecessary when we have instancing,
    // because it is used to convert a quad to 4 vertices which we won't need to do.
    // It packs position + UV into u32 for the GPU shader.
    pub fn vertices_packed(&self, xyz: u32, w: u32, h: u32, lod: u32) -> [u32; 4] {
        let xyz = xyz * lod;
        let w_ = w * lod;
        let h_ = h * lod;
        match self {
            Face::Left => [
                vertex_info(xyz, h, w),
                vertex_info(xyz + packed_xyz(0, 0, h_), 0, w),
                vertex_info(xyz + packed_xyz(0, w_, 0), h, 0),
                vertex_info(xyz + packed_xyz(0, w_, h_), 0, 0),
            ],
            Face::Down => [
                vertex_info(xyz - packed_xyz(w_, 0, 0) + packed_xyz(0, 0, h_), w, h),
                vertex_info(xyz - packed_xyz(w_, 0, 0), w, 0),
                vertex_info(xyz + packed_xyz(0, 0, h_), 0, h),
                vertex_info(xyz, 0, 0),
            ],
            Face::Back => [
                vertex_info(xyz, w, h),
                vertex_info(xyz + packed_xyz(0, h_, 0), w, 0),
                vertex_info(xyz + packed_xyz(w_, 0, 0), 0, h),
                vertex_info(xyz + packed_xyz(w_, h_, 0), 0, 0),
            ],
            Face::Right => [
                vertex_info(xyz, 0, 0),
                vertex_info(xyz + packed_xyz(0, 0, h_), h, 0),
                vertex_info(xyz - packed_xyz(0, w_, 0), 0, w),
                vertex_info(xyz + packed_xyz(0, 0, h_) - packed_xyz(0, w_, 0), h, w),
            ],
            Face::Up => [
                vertex_info(xyz + packed_xyz(w_, 0, h_), w, h),
                vertex_info(xyz + packed_xyz(w_, 0, 0), w, 0),
                vertex_info(xyz + packed_xyz(0, 0, h_), 0, h),
                vertex_info(xyz, 0, 0),
            ],
            Face::Front => [
                vertex_info(xyz - packed_xyz(w_, 0, 0) + packed_xyz(0, h_, 0), 0, 0),
                vertex_info(xyz - packed_xyz(w_, 0, 0), 0, h),
                vertex_info(xyz + packed_xyz(0, h_, 0), w, 0),
                vertex_info(xyz, w, h),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compute the normal of a triangle using the cross product.
    /// For counter-clockwise winding (when viewed from the front), this gives the outward normal.
    fn triangle_normal(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) -> [f32; 3] {
        // Edge vectors
        let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

        // Cross product e1 × e2
        [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ]
    }

    /// Normalize a vector
    fn normalize(v: [f32; 3]) -> [f32; 3] {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len > 0.0 {
            [v[0] / len, v[1] / len, v[2] / len]
        } else {
            v
        }
    }

    /// Test that the winding order produces normals matching Parry3d/Avian3d convention.
    ///
    /// Parry3d uses counter-clockwise winding for outward-facing normals.
    /// For a block at position (x, y, z):
    /// - Face::Right should have normal pointing in +X direction
    /// - Face::Left should have normal pointing in -X direction
    /// - Face::Up should have normal pointing in +Y direction
    /// - Face::Down should have normal pointing in -Y direction
    /// - Face::Front should have normal pointing in +Z direction
    /// - Face::Back should have normal pointing in -Z direction
    #[test]
    fn test_winding_order_produces_correct_outward_normals() {
        use strum::IntoEnumIterator;

        let mut failures = Vec::new();

        for face in Face::iter() {
            // Get vertices for a 1x1 quad at origin
            let verts = face.quad_vertices(0.0, 0.0, 0.0, 1.0, 1.0);
            let (tri1_indices, _tri2_indices) = face.triangle_indices(0);

            // Get the vertices for the first triangle
            let v0 = verts[tri1_indices[0] as usize];
            let v1 = verts[tri1_indices[1] as usize];
            let v2 = verts[tri1_indices[2] as usize];

            // Compute the normal from the winding
            let computed_normal = normalize(triangle_normal(v0, v1, v2));

            // Expected normal direction for each face
            let expected_normal: [f32; 3] = match face {
                Face::Right => [1.0, 0.0, 0.0],
                Face::Left => [-1.0, 0.0, 0.0],
                Face::Up => [0.0, 1.0, 0.0],
                Face::Down => [0.0, -1.0, 0.0],
                Face::Front => [0.0, 0.0, 1.0],
                Face::Back => [0.0, 0.0, -1.0],
            };

            // Check that the computed normal matches expected (with some tolerance)
            let dot = computed_normal[0] * expected_normal[0]
                + computed_normal[1] * expected_normal[1]
                + computed_normal[2] * expected_normal[2];

            if dot < 0.99 {
                failures.push(format!(
                    "Face {:?}: computed normal {:?} does not match expected {:?} (dot product: {})",
                    face, computed_normal, expected_normal, dot
                ));
            }
        }

        if !failures.is_empty() {
            panic!("Winding order issues:\n{}", failures.join("\n"));
        }
    }

    /// Test that both triangles of a quad have consistent normals
    #[test]
    fn test_both_triangles_have_consistent_normals() {
        use strum::IntoEnumIterator;

        for face in Face::iter() {
            let verts = face.quad_vertices(0.0, 0.0, 0.0, 1.0, 1.0);
            let (tri1_indices, tri2_indices) = face.triangle_indices(0);

            // First triangle normal
            let v0 = verts[tri1_indices[0] as usize];
            let v1 = verts[tri1_indices[1] as usize];
            let v2 = verts[tri1_indices[2] as usize];
            let normal1 = normalize(triangle_normal(v0, v1, v2));

            // Second triangle normal
            let v0 = verts[tri2_indices[0] as usize];
            let v1 = verts[tri2_indices[1] as usize];
            let v2 = verts[tri2_indices[2] as usize];
            let normal2 = normalize(triangle_normal(v0, v1, v2));

            // Both normals should point in the same direction
            let dot = normal1[0] * normal2[0] + normal1[1] * normal2[1] + normal1[2] * normal2[2];

            assert!(
                dot > 0.99,
                "Face {:?}: triangle normals are inconsistent. Triangle 1: {:?}, Triangle 2: {:?}",
                face,
                normal1,
                normal2
            );
        }
    }

    /// Test that Face::n() returns the expected normal direction
    #[test]
    fn test_face_n_matches_winding_normal() {
        use strum::IntoEnumIterator;

        for face in Face::iter() {
            let verts = face.quad_vertices(0.0, 0.0, 0.0, 1.0, 1.0);
            let (tri1_indices, _) = face.triangle_indices(0);

            let v0 = verts[tri1_indices[0] as usize];
            let v1 = verts[tri1_indices[1] as usize];
            let v2 = verts[tri1_indices[2] as usize];
            let computed_normal = normalize(triangle_normal(v0, v1, v2));

            let face_n = face.n();
            let expected = [face_n[0] as f32, face_n[1] as f32, face_n[2] as f32];

            let dot = computed_normal[0] * expected[0]
                + computed_normal[1] * expected[1]
                + computed_normal[2] * expected[2];

            assert!(
                dot > 0.99,
                "Face {:?}: winding normal {:?} does not match Face::n() {:?}",
                face,
                computed_normal,
                expected
            );
        }
    }
}
