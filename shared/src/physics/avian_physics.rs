//! Avian3d-based physics simulation for voxel worlds.
//!
//! This module provides physics simulation using avian3d with chunk-level
//! trimesh colliders for the voxel world. Players use dynamic rigid bodies
//! and avian3d handles collision detection and response.
//!
//! Key features:
//! - Uses avian3d `RigidBody::Dynamic` for players with capsule colliders
//! - Chunk colliders are `RigidBody::Static` trimeshes (see meshing module)
//! - Custom kinematics: we control velocity directly, avian3d resolves collisions
//! - Supports both walking and flying movement modes
//! - Server-authoritative design with client-side prediction support

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::block::Block;
use crate::world::block_access::BlockAccess;
use crate::world::pos::pos3d::BlockPos;
use crate::world::realm::Realm;
use crate::{FLY_SPEED, FLY_VERTICAL_SPEED, WALK_SPEED};

/// Player physics constants
pub const PLAYER_GRAVITY: f32 = 50.0;
pub const PLAYER_JUMP_FORCE: f32 = 13.0;
/// Player capsule collider dimensions
pub const PLAYER_CAPSULE_RADIUS: f32 = 0.3;
pub const PLAYER_CAPSULE_HEIGHT: f32 = 1.1; // Total height = height + 2*radius = 1.7
/// Player query bounds for block lookups below the player (width, height, depth)
pub const PLAYER_QUERY_BOUNDS: Vec3 = Vec3::new(0.6, 1.7, 0.6);
/// Base acceleration rate for ground movement (units/s² per unit of friction)
pub const GROUND_ACCELERATION: f32 = 150.0;
/// Friction coefficient when on ground (higher = more responsive)
pub const GROUND_FRICTION: f32 = 8.0;
/// Friction coefficient when in air (lower = less air control)
pub const AIR_FRICTION: f32 = 2.0;

/// Represents the movement mode of an entity.
///
/// This is now used directly as a component on player entities, replacing
/// the previous `Walking`/`Flying` marker components.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum MovementMode {
    #[default]
    Walking,
    Flying,
}

impl MovementMode {
    pub fn speed(&self) -> f32 {
        match self {
            MovementMode::Walking => WALK_SPEED,
            MovementMode::Flying => FLY_SPEED,
        }
    }
}

/// Input state for a single physics tick.
/// 
/// This struct captures all input needed for one frame of physics simulation.
#[derive(Debug, Clone, Default)]
pub struct MovementInput {
    /// Horizontal movement direction (normalized), relative to camera
    pub move_direction: Vec3,
    /// Whether the jump/fly-up input is pressed
    pub jump: bool,
    /// Whether the crouch/fly-down input is pressed
    pub crouch: bool,
    /// Camera forward direction (for movement orientation)
    pub camera_forward: Vec3,
    /// Camera right direction (for movement orientation)
    pub camera_right: Vec3,
}

/// Physics state for an entity using avian3d types.
///
/// This is the canonical physics state used for simulation. It's compatible
/// with avian3d's Position and LinearVelocity components.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PhysicsState {
    /// Entity position (compatible with avian3d Position)
    pub position: Vec3,
    /// Entity velocity (compatible with avian3d LinearVelocity)
    pub velocity: Vec3,
    /// Current movement mode (walking or flying)
    pub movement_mode: MovementMode,
    /// The realm the entity is in
    pub realm: Realm,
    /// Whether the entity is on the ground (for jumping)
    pub on_ground: bool,
}

impl PhysicsState {
    /// Create a new physics state at the given position
    pub fn new(position: Vec3, realm: Realm) -> Self {
        Self {
            position,
            velocity: Vec3::ZERO,
            movement_mode: MovementMode::Walking,
            realm,
            on_ground: false,
        }
    }

    /// Build a physics state from components (common pattern in systems)
    pub fn from_components(
        position: Vec3,
        velocity: Vec3,
        movement_mode: MovementMode,
        realm: Realm,
        on_ground: bool,
    ) -> Self {
        Self {
            position,
            velocity,
            movement_mode,
            realm,
            on_ground,
        }
    }

    /// Convert to avian3d Position component
    pub fn to_position(&self) -> Position {
        Position(self.position.into())
    }

    /// Convert to avian3d LinearVelocity component
    pub fn to_linear_velocity(&self) -> LinearVelocity {
        LinearVelocity(self.velocity.into())
    }
}

/// Result of computing desired velocity for a single physics tick.
/// 
/// This is the low-level output from `compute_desired_velocity`. For the
/// higher-level player input result (including movement mode changes),
/// see `PlayerStepOutput` in the `player_step` module.
#[derive(Debug, Clone)]
pub struct PhysicsStepResult {
    pub new_velocity: Vec3,
}

/// Avian3d player physics bundle with dynamic rigid body.
///
/// Uses a capsule collider for smooth movement over terrain.
/// Avian3d handles collision detection and response against chunk colliders.
/// We control movement by setting LinearVelocity directly (custom kinematics).
#[derive(Bundle)]
pub struct PlayerPhysicsBundle {
    pub rigid_body: RigidBody,
    pub collider: Collider,
    pub linear_velocity: LinearVelocity,
    pub angular_velocity: AngularVelocity,
    pub locked_axes: LockedAxes,
    pub gravity_scale: GravityScale,
    pub friction: Friction,
    pub restitution: Restitution,
    pub ccd: SweptCcd,
}

impl PlayerPhysicsBundle {
    /// Create a new player physics bundle.
    ///
    /// Uses a dynamic rigid body with capsule collider. Avian3d will handle
    /// collision detection against chunk trimesh colliders.
    pub fn new() -> Self {
        Self {
            rigid_body: RigidBody::Dynamic,
            collider: Collider::capsule(PLAYER_CAPSULE_RADIUS, PLAYER_CAPSULE_HEIGHT),
            linear_velocity: LinearVelocity::default(),
            angular_velocity: AngularVelocity::default(),
            // Lock all rotation to prevent tipping over
            locked_axes: LockedAxes::ROTATION_LOCKED,
            // Use standard gravity (configured via Gravity resource)
            gravity_scale: GravityScale(1.0),
            // Low friction for responsive movement
            friction: Friction::new(0.1),
            // No bounce
            restitution: Restitution::new(0.0),
            // Enable continuous collision detection for fast movement
            ccd: SweptCcd::default(),
        }
    }
}

impl Default for PlayerPhysicsBundle {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Block Query Functions (for gameplay logic, not physics collision)
// =============================================================================
// These functions query the voxel world directly for gameplay purposes like
// footstep sounds and surface friction. For physics collision/ground detection,
// use `is_on_ground_from_contacts` which queries avian3d's collision data.
// =============================================================================

/// Get block positions below the player for ground detection.
fn blocks_below(pos: Vec3, realm: Realm, aabb: Vec3) -> impl Iterator<Item = BlockPos> {
    let y = (pos.y - 0.01).floor() as i32;
    let x_start = pos.x.floor() as i32;
    let x_end = (pos.x + aabb.x).floor() as i32;
    let z_start = pos.z.floor() as i32;
    let z_end = (pos.z + aabb.z).floor() as i32;

    (x_start..=x_end)
        .flat_map(move |x| (z_start..=z_end).map(move |z| (x, z)))
        .map(move |(x, z)| BlockPos { x, y, z, realm })
}

/// Check if the entity is standing on a solid block (for gameplay effects).
///
/// This queries the voxel world directly - use it for:
/// - Footstep sound triggers
/// - Surface-specific effects
/// - Block interaction availability
///
/// **For physics ground detection** (jumping, gravity), use `OnGround` component
/// which is updated from avian3d's collision contacts.
pub fn check_on_ground<W: BlockAccess>(
    world: &W,
    position: Vec3,
    realm: Realm,
    aabb: Vec3,
) -> bool {
    for block_pos in blocks_below(position, realm, aabb) {
        let block = world.get_block_safe(block_pos);
        if !block.is_traversable() {
            return true;
        }
    }
    false
}

/// Get the block the entity is standing on (for friction/slowing calculations)
pub fn get_stepped_block<W: BlockAccess>(
    world: &W,
    position: Vec3,
    realm: Realm,
    aabb: Vec3,
) -> Block {
    let mut closest_block = Block::Air;
    let mut min_dist = f32::INFINITY;
    for block_pos in blocks_below(position, realm, aabb) {
        let block = world.get_block_safe(block_pos);
        if block.is_traversable() {
            continue;
        }
        let dist = (position.x - block_pos.x as f32).abs() + (position.z - block_pos.z as f32).abs();
        if dist < min_dist {
            min_dist = dist;
            closest_block = block;
        }
    }
    closest_block
}

// =============================================================================
// Velocity Computation (Custom Kinematics)
// =============================================================================

/// Compute the desired velocity for a player based on input and current state.
///
/// This implements "custom kinematics" - we compute what velocity we want,
/// and avian3d handles collision resolution against chunk colliders.
///
/// For walking mode:
/// - Horizontal velocity is computed from input with acceleration/friction
/// - Vertical velocity is passed through (avian3d applies gravity)
/// - Jump impulse is applied when grounded and jump is pressed
///
/// For flying mode:
/// - Direct velocity control, no gravity
pub fn compute_desired_velocity(
    state: &PhysicsState,
    input: &MovementInput,
    delta_seconds: f32,
) -> PhysicsStepResult {
    let mut velocity = state.velocity;

    // Calculate world-space movement direction from input
    let forward_horizontal =
        Vec3::new(input.camera_forward.x, 0.0, input.camera_forward.z).normalize_or_zero();
    let right_horizontal =
        Vec3::new(input.camera_right.x, 0.0, input.camera_right.z).normalize_or_zero();

    // Transform local movement direction to world space
    let world_move_dir = if input.move_direction.length_squared() > 0.0 {
        let local_dir = input.move_direction.normalize();
        forward_horizontal * local_dir.z + right_horizontal * local_dir.x
    } else {
        Vec3::ZERO
    };

    // Calculate target horizontal velocity from input direction and mode speed
    let speed = state.movement_mode.speed();
    let target_velocity_xz = world_move_dir * speed;

    match state.movement_mode {
        MovementMode::Flying => {
            // Flying mode: direct velocity control
            velocity.x = target_velocity_xz.x;
            velocity.z = target_velocity_xz.z;
            velocity.y = (input.jump as i32 - input.crouch as i32) as f32 * FLY_VERTICAL_SPEED;

            PhysicsStepResult {
                new_velocity: velocity,
            }
        }
        MovementMode::Walking => {
            // Handle jumping (apply impulse if grounded)
            if input.jump && state.on_ground {
                velocity.y = PLAYER_JUMP_FORCE;
            }

            // Use ground or air friction based on contact state
            // (Future: could query stepped block for surface-specific friction)
            let friction = if state.on_ground {
                GROUND_FRICTION
            } else {
                AIR_FRICTION
            };

            // Smoothly accelerate horizontal velocity towards target
            let velocity_diff = Vec3::new(
                target_velocity_xz.x - velocity.x,
                0.0,
                target_velocity_xz.z - velocity.z,
            );

            let diff_magnitude = velocity_diff.length();
            if diff_magnitude > 0.0 {
                let accel_factor = (delta_seconds * friction * GROUND_ACCELERATION / diff_magnitude.max(1.0)).min(1.0);
                velocity.x += velocity_diff.x * accel_factor;
                velocity.z += velocity_diff.z * accel_factor;
            }

            PhysicsStepResult {
                new_velocity: velocity,
            }
        }
    }
}

/// Convert transmittable actions to movement input
pub fn actions_to_movement_input(
    inputs: &bevy::platform::collections::HashSet<crate::messages::TransmittableAction>,
    camera_transform: &Transform,
) -> MovementInput {
    use crate::messages::TransmittableAction;

    let forward = camera_transform.forward().as_vec3();
    let right = camera_transform.right().as_vec3();

    let mut move_direction = Vec3::ZERO;
    let mut jump = false;
    let mut crouch = false;

    for action in inputs {
        match action {
            TransmittableAction::MoveForward => move_direction.z += 1.0,
            TransmittableAction::MoveBackward => move_direction.z -= 1.0,
            TransmittableAction::MoveRight => move_direction.x += 1.0,
            TransmittableAction::MoveLeft => move_direction.x -= 1.0,
            TransmittableAction::JumpOrFlyUp => jump = true,
            TransmittableAction::CrouchOrFlyDown => crouch = true,
            _ => {}
        }
    }

    MovementInput {
        move_direction,
        jump,
        crouch,
        camera_forward: forward,
        camera_right: right,
    }
}

// =============================================================================
// Movement Mode Marker Components (Deprecated)
// =============================================================================
// These marker components are deprecated in favor of using `MovementMode`
// directly as a component. They are kept temporarily for backward compatibility
// during migration but should be removed once all code uses `MovementMode`.
// =============================================================================

/// Marker component for walking movement mode.
/// 
/// **Deprecated**: Use `MovementMode::Walking` component instead.
#[derive(Component)]
#[deprecated(note = "Use MovementMode component directly")]
pub struct Walking;

/// Marker component for flying movement mode.
/// 
/// **Deprecated**: Use `MovementMode::Flying` component instead.
#[derive(Component)]
#[deprecated(note = "Use MovementMode component directly")]
pub struct Flying;

/// Sync movement mode marker components on an entity.
///
/// **Deprecated**: This function syncs the old marker components. New code
/// should use `MovementMode` as a component directly and mutate it.
#[deprecated(note = "Use MovementMode component directly")]
pub fn sync_movement_mode_components(
    commands: &mut Commands,
    entity: Entity,
    new_mode: MovementMode,
    was_flying: bool,
) {
    let new_is_flying = new_mode == MovementMode::Flying;
    if new_is_flying != was_flying {
        if new_is_flying {
            commands.entity(entity).remove::<Walking>().insert(Flying);
        } else {
            commands.entity(entity).remove::<Flying>().insert(Walking);
        }
    }
}

// =============================================================================
// Avian3d Plugin Integration
// =============================================================================

/// Shared physics plugin for avian3d integration.
///
/// This plugin sets up avian3d for voxel world physics:
/// - Players use `RigidBody::Dynamic` with capsule colliders
/// - Chunk terrain uses `RigidBody::Static` with trimesh colliders
/// - Avian3d handles collision detection and response
#[derive(Default)]
pub struct SharedPhysicsPlugin;

impl Plugin for SharedPhysicsPlugin {
    fn build(&self, app: &mut App) {
        use avian3d::prelude::*;

        // Use full physics plugins for collision detection on both client and server
        // The server now includes AssetPlugin so it can handle mesh colliders
        app.add_plugins(PhysicsPlugins::default().with_length_unit(1.0));

        // Configure gravity for the world
        app.insert_resource(Gravity(Vec3::new(0.0, -PLAYER_GRAVITY, 0.0).into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock world for testing
    struct TestWorld;

    impl BlockAccess for TestWorld {
        fn get_block_safe(&self, pos: BlockPos) -> Block {
            // Floor at y=0
            if pos.y < 0 {
                Block::Granite
            } else {
                Block::Air
            }
        }

        fn is_chunk_loaded(&self, _chunk_pos: crate::world::pos::pos3d::ChunkPos) -> bool {
            true
        }
    }

    #[test]
    fn test_physics_state_avian_conversion() {
        let state = PhysicsState {
            position: Vec3::new(1.0, 2.0, 3.0),
            velocity: Vec3::new(0.5, 1.0, -0.5),
            movement_mode: MovementMode::Walking,
            realm: Realm::Overworld,
            on_ground: true,
        };

        let pos = state.to_position();
        let vel = state.to_linear_velocity();

        assert_eq!(Vec3::from(pos.0), state.position);
        assert_eq!(Vec3::from(vel.0), state.velocity);
    }

    #[test]
    fn test_ground_detection() {
        let world = TestWorld;

        // Player at y=0 should be on ground (floor at y=-1)
        let on_ground = check_on_ground(&world, Vec3::new(0.0, 0.0, 0.0), Realm::Overworld, PLAYER_QUERY_BOUNDS);
        assert!(on_ground);

        // Player at y=5 should not be on ground
        let on_ground = check_on_ground(&world, Vec3::new(0.0, 5.0, 0.0), Realm::Overworld, PLAYER_QUERY_BOUNDS);
        assert!(!on_ground);
    }

    #[test]
    fn test_velocity_computation_walking() {
        let state = PhysicsState {
            position: Vec3::new(0.0, 10.0, 0.0),
            velocity: Vec3::ZERO,
            movement_mode: MovementMode::Walking,
            realm: Realm::Overworld,
            on_ground: true,
        };
        let input = MovementInput {
            move_direction: Vec3::new(0.0, 0.0, 1.0), // Forward
            jump: false,
            crouch: false,
            camera_forward: Vec3::Z,
            camera_right: Vec3::X,
        };

        let result = compute_desired_velocity(&state, &input, 0.1);

        // Should have positive Z velocity (forward movement)
        assert!(result.new_velocity.z > 0.0);
    }

    #[test]
    fn test_velocity_computation_jump() {
        let state = PhysicsState {
            position: Vec3::new(0.0, 0.0, 0.0),
            velocity: Vec3::ZERO,
            movement_mode: MovementMode::Walking,
            realm: Realm::Overworld,
            on_ground: true,
        };
        let input = MovementInput {
            move_direction: Vec3::ZERO,
            jump: true,
            crouch: false,
            camera_forward: Vec3::Z,
            camera_right: Vec3::X,
        };

        let result = compute_desired_velocity(&state, &input, 0.1);

        // Should have upward velocity from jump
        assert_eq!(result.new_velocity.y, PLAYER_JUMP_FORCE);
    }

    #[test]
    fn test_flying_mode_direct_control() {
        let state = PhysicsState {
            position: Vec3::new(0.0, 10.0, 0.0),
            velocity: Vec3::ZERO,
            movement_mode: MovementMode::Flying,
            realm: Realm::Overworld,
            on_ground: false,
        };
        let input = MovementInput {
            move_direction: Vec3::ZERO,
            jump: true, // Fly up
            crouch: false,
            camera_forward: Vec3::Z,
            camera_right: Vec3::X,
        };

        let result = compute_desired_velocity(&state, &input, 0.1);

        // Should have upward velocity
        assert_eq!(result.new_velocity.y, FLY_VERTICAL_SPEED);
    }
}
