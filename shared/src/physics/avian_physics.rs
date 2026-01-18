//! Avian3d-based physics simulation for voxel worlds.
//!
//! This module provides physics simulation using avian3d while integrating
//! with the voxel-based world collision system. It maintains API compatibility
//! with the existing physics system to minimize changes to client/server code.
//!
//! Key features:
//! - Uses avian3d types for physics state (Position, LinearVelocity, etc.)
//! - Custom collision detection against voxel world via BlockAccess trait
//! - Supports both walking and flying movement modes
//! - Server-authoritative design with client-side prediction support

use avian3d::prelude::*;
use bevy::prelude::*;
use itertools::iproduct;

use crate::block::Block;
use crate::world::block_access::BlockAccess;
use crate::world::pos::pos3d::BlockPos;
use crate::world::realm::Realm;
use crate::{FLY_SPEED, FLY_VERTICAL_SPEED, WALK_SPEED};

/// Player physics constants
pub const PLAYER_GRAVITY: f32 = 50.0;
pub const PLAYER_JUMP_FORCE: f32 = 13.0;
/// Player collision box half-extents (avian3d uses half-extents for cuboids)
pub const PLAYER_HALF_EXTENTS: Vec3 = Vec3::new(0.25, 0.85, 0.25);
/// Player AABB full size for block collision checks
pub const PLAYER_AABB: Vec3 = Vec3::new(0.5, 1.7, 0.5);
/// Acceleration multiplier for ground movement
pub const ACC_MULT: f32 = 150.0;

/// Represents the movement mode of an entity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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

    /// Convert to avian3d Position component
    pub fn to_position(&self) -> Position {
        Position(self.position.into())
    }

    /// Convert to avian3d LinearVelocity component
    pub fn to_linear_velocity(&self) -> LinearVelocity {
        LinearVelocity(self.velocity.into())
    }

    /// Update from avian3d Position component
    pub fn set_from_position(&mut self, pos: &Position) {
        self.position = pos.0.into();
    }

    /// Update from avian3d LinearVelocity component
    pub fn set_from_linear_velocity(&mut self, vel: &LinearVelocity) {
        self.velocity = vel.0.into();
    }
}

/// Result of a physics simulation step
#[derive(Debug, Clone)]
pub struct PhysicsStepResult {
    pub new_position: Vec3,
    pub new_velocity: Vec3,
    pub on_ground: bool,
}

/// Avian3d-compatible player collider bundle.
///
/// This provides the components needed for avian3d character physics.
/// Note: We don't use avian3d's built-in collision detection against the world
/// because we have a voxel world that requires custom BlockAccess queries.
#[derive(Bundle)]
pub struct PlayerPhysicsBundle {
    pub position: Position,
    pub rotation: Rotation,
    pub linear_velocity: LinearVelocity,
    pub collider: Collider,
    pub rigid_body: RigidBody,
    pub locked_axes: LockedAxes,
    pub gravity_scale: GravityScale,
}

impl PlayerPhysicsBundle {
    /// Create a new player physics bundle at the given position.
    ///
    /// We use a kinematic rigid body because we handle all physics manually
    /// via the custom voxel collision system.
    pub fn new(position: Vec3) -> Self {
        Self {
            position: Position(position.into()),
            rotation: Rotation::default(),
            linear_velocity: LinearVelocity::default(),
            collider: Collider::cuboid(
                PLAYER_HALF_EXTENTS.x * 2.0,
                PLAYER_HALF_EXTENTS.y * 2.0,
                PLAYER_HALF_EXTENTS.z * 2.0,
            ),
            // Kinematic because we do manual collision resolution
            rigid_body: RigidBody::Kinematic,
            // Lock rotation to prevent tipping over
            locked_axes: LockedAxes::ROTATION_LOCKED,
            // We handle gravity manually for voxel collision
            gravity_scale: GravityScale(0.0),
        }
    }
}

// =============================================================================
// Voxel Collision Detection Functions
// =============================================================================

/// Compute the extent of positions to check for a given coordinate and size
fn extent(v: f32, size: f32) -> Vec<i32> {
    let start = v.floor() as i32;
    let end = (size + v).floor() as i32;
    if size > 0.0 {
        (start..=end).collect()
    } else {
        (end..=start).rev().collect()
    }
}

/// Get block positions perpendicular to the Y axis (for Y collision checks)
fn blocks_perp_y(pos: Vec3, realm: Realm, aabb: Vec3) -> impl Iterator<Item = BlockPos> {
    iproduct!(extent(pos.x, aabb.x), extent(pos.z, aabb.z)).map(move |(x, z)| BlockPos {
        x,
        y: pos.y.floor() as i32,
        z,
        realm,
    })
}

/// Get block positions perpendicular to the Z axis (for Z collision checks)
fn blocks_perp_z(pos: Vec3, realm: Realm, aabb: Vec3) -> impl Iterator<Item = BlockPos> {
    iproduct!(extent(pos.x, aabb.x), extent(pos.y, aabb.y)).map(move |(x, y)| BlockPos {
        x,
        y,
        z: pos.z.floor() as i32,
        realm,
    })
}

/// Get block positions perpendicular to the X axis (for X collision checks)
fn blocks_perp_x(pos: Vec3, realm: Realm, aabb: Vec3) -> impl Iterator<Item = BlockPos> {
    iproduct!(extent(pos.y, aabb.y), extent(pos.z, aabb.z)).map(move |(y, z)| BlockPos {
        x: pos.x.floor() as i32,
        y,
        z,
        realm,
    })
}

/// Check if the entity is standing on solid ground
pub fn check_on_ground<W: BlockAccess>(
    world: &W,
    position: Vec3,
    realm: Realm,
    aabb: Vec3,
) -> bool {
    let below = position + Vec3::new(0., -0.01, 0.);
    for block_pos in blocks_perp_y(below, realm, aabb) {
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
    let below = position + Vec3::new(0., -0.01, 0.);
    let mut closest_block = Block::Air;
    let mut min_dist = f32::INFINITY;
    for block_pos in blocks_perp_y(below, realm, aabb) {
        let block = world.get_block_safe(block_pos);
        if block.is_traversable() {
            continue;
        }
        let dist = (below.x - block_pos.x as f32).abs() - (below.y - block_pos.y as f32).abs();
        if dist < min_dist {
            min_dist = dist;
            closest_block = block;
        }
    }
    closest_block
}

// =============================================================================
// Physics Simulation
// =============================================================================

/// Simulate one physics step for an entity.
///
/// This is the core physics simulation that both client and server use.
/// It handles gravity, collision detection, and movement using avian3d-compatible
/// types while performing custom voxel collision detection.
///
/// # Arguments
/// * `world` - Block access for collision detection
/// * `state` - Current physics state
/// * `input` - Movement input for this frame
/// * `delta_seconds` - Time step in seconds
///
/// # Returns
/// The result of the physics step with new position, velocity, and ground state.
pub fn simulate_physics_step<W: BlockAccess>(
    world: &W,
    state: &PhysicsState,
    input: &MovementInput,
    delta_seconds: f32,
) -> PhysicsStepResult {
    let aabb = PLAYER_AABB;
    let mut position = state.position;
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

    // Calculate heading (desired velocity)
    let speed = state.movement_mode.speed();
    let heading = world_move_dir * speed;

    match state.movement_mode {
        MovementMode::Flying => {
            // Flying mode: direct velocity control, no collision
            velocity.x = heading.x;
            velocity.z = heading.z;
            velocity.y = (input.jump as i32 - input.crouch as i32) as f32 * FLY_VERTICAL_SPEED;

            // In flying mode, skip collision detection entirely
            position += velocity * delta_seconds;

            PhysicsStepResult {
                new_position: position,
                new_velocity: velocity,
                on_ground: false,
            }
        }
        MovementMode::Walking => {
            // Apply gravity using avian3d-style integration
            velocity.y -= PLAYER_GRAVITY * delta_seconds;

            // Check if on ground for jumping
            let on_ground = check_on_ground(world, position, state.realm, aabb);

            // Handle jumping
            if input.jump && on_ground {
                velocity.y = PLAYER_JUMP_FORCE;
            }

            // Get stepped block for friction/slowing
            let stepped_block = get_stepped_block(world, position, state.realm, aabb);
            let friction = stepped_block.friction();
            let slowing = stepped_block.slowing();

            // Apply slowing to heading
            let slowed_heading = Vec3::new(heading.x * slowing, f32::NAN, heading.z * slowing);

            // Make velocity inch towards heading (X and Z only)
            let diff = Vec3::new(
                slowed_heading.x - velocity.x,
                0.0,
                slowed_heading.z - velocity.z,
            );

            let diff_len = diff.length();
            if diff_len > 0.0 {
                let c = (delta_seconds * friction * ACC_MULT / diff_len.max(1.0)).min(1.0);
                let acc = c * diff;
                velocity.x += acc.x;
                velocity.z += acc.z;
            }

            // Apply velocity with collision detection
            let (new_position, new_velocity, on_ground) = apply_velocity_with_collision(
                world,
                position,
                velocity,
                state.realm,
                aabb,
                delta_seconds,
            );

            PhysicsStepResult {
                new_position,
                new_velocity,
                on_ground,
            }
        }
    }
}

/// Apply velocity to position with collision detection against voxel world.
///
/// This performs swept AABB collision detection against the block world,
/// resolving collisions on each axis independently (X, then Y, then Z).
fn apply_velocity_with_collision<W: BlockAccess>(
    world: &W,
    mut position: Vec3,
    mut velocity: Vec3,
    realm: Realm,
    aabb: Vec3,
    delta_seconds: f32,
) -> (Vec3, Vec3, bool) {
    let applied_velocity = velocity * delta_seconds;
    let mut on_ground = false;

    // X axis collision
    let xpos = if applied_velocity.x > 0. {
        aabb.x + position.x
    } else {
        position.x
    };
    let mut stopped = false;
    for x in extent(xpos, applied_velocity.x).into_iter().skip(1) {
        let pos_x = Vec3 {
            x: x as f32,
            y: position.y,
            z: position.z,
        };
        if blocks_perp_x(pos_x, realm, aabb).any(|pos| !world.get_block_safe(pos).is_traversable())
        {
            if applied_velocity.x > 0. {
                position.x = pos_x.x - aabb.x - 0.001;
            } else {
                position.x = pos_x.x + 1.001;
            }
            velocity.x = 0.;
            stopped = true;
            break;
        }
    }
    if !stopped {
        position.x += applied_velocity.x;
    }

    // Y axis collision
    let ypos = if applied_velocity.y > 0. {
        aabb.y + position.y
    } else {
        position.y
    };
    let mut stopped = false;
    for y in extent(ypos, applied_velocity.y).into_iter().skip(1) {
        let pos_y = Vec3 {
            x: position.x,
            y: y as f32,
            z: position.z,
        };
        if blocks_perp_y(pos_y, realm, aabb).any(|pos| !world.get_block_safe(pos).is_traversable())
        {
            if applied_velocity.y > 0. {
                position.y = pos_y.y - aabb.y - 0.001;
            } else {
                position.y = pos_y.y + 1.001;
                on_ground = true;
            }
            velocity.y = 0.;
            stopped = true;
            break;
        }
    }
    if !stopped {
        position.y += applied_velocity.y;
    }

    // Z axis collision
    let zpos = if applied_velocity.z > 0. {
        aabb.z + position.z
    } else {
        position.z
    };
    let mut stopped = false;
    for z in extent(zpos, applied_velocity.z).into_iter().skip(1) {
        let pos_z = Vec3 {
            x: position.x,
            y: position.y,
            z: z as f32,
        };
        if blocks_perp_z(pos_z, realm, aabb).any(|pos| !world.get_block_safe(pos).is_traversable())
        {
            if applied_velocity.z > 0. {
                position.z = pos_z.z - aabb.z - 0.001;
            } else {
                position.z = pos_z.z + 1.001;
            }
            velocity.z = 0.;
            stopped = true;
            break;
        }
    }
    if !stopped {
        position.z += applied_velocity.z;
    }

    (position, velocity, on_ground)
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
// Avian3d Plugin Integration
// =============================================================================

/// Shared physics plugin for avian3d integration.
///
/// This plugin sets up avian3d with settings appropriate for voxel world physics.
/// Note that actual collision detection is done via the custom BlockAccess system,
/// not avian3d's built-in collision detection.
///
/// Use `SharedPhysicsPlugin::client()` for client apps (with debug rendering support)
/// or `SharedPhysicsPlugin::server()` for headless server apps.
pub struct SharedPhysicsPlugin {
    /// Whether this is running on a headless server (no rendering)
    headless: bool,
}

impl SharedPhysicsPlugin {
    /// Create a physics plugin configured for client use (with debug rendering support)
    pub fn client() -> Self {
        Self { headless: false }
    }

    /// Create a physics plugin configured for headless server use
    pub fn server() -> Self {
        Self { headless: true }
    }
}

impl Default for SharedPhysicsPlugin {
    fn default() -> Self {
        Self::client()
    }
}

impl Plugin for SharedPhysicsPlugin {
    fn build(&self, app: &mut App) {
        use avian3d::prelude::*;
        use avian3d::schedule::PhysicsSchedulePlugin;

        if self.headless {
            // For headless server: we only need the physics scheduling infrastructure,
            // not the full collision system (which requires AssetPlugin for mesh colliders).
            // Since we use custom voxel-based collision via BlockAccess, we set up
            // a minimal configuration with just the scheduling.
            //
            // This avoids ColliderCachePlugin which requires AssetEvent<Mesh>.
            app.add_plugins(PhysicsSchedulePlugin::new(FixedUpdate));

            // Register the core physics types we use
            app.register_type::<RigidBody>()
                .register_type::<Position>()
                .register_type::<Rotation>()
                .register_type::<LinearVelocity>()
                .register_type::<AngularVelocity>()
                .register_type::<GravityScale>()
                .register_type::<LockedAxes>();
        } else {
            // For client: use default physics plugins (includes debug rendering support)
            // The client has AssetPlugin so full collision features work
            app.add_plugins(PhysicsPlugins::default().with_length_unit(1.0));
        }

        // Configure gravity (we handle it manually for voxel collision, but set default for reference)
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
        
        // Player at y=1 should be on ground (floor at y=0)
        let on_ground = check_on_ground(&world, Vec3::new(0.0, 1.0, 0.0), Realm::Overworld, PLAYER_AABB);
        assert!(on_ground);

        // Player at y=5 should not be on ground
        let on_ground = check_on_ground(&world, Vec3::new(0.0, 5.0, 0.0), Realm::Overworld, PLAYER_AABB);
        assert!(!on_ground);
    }

    #[test]
    fn test_physics_step_gravity() {
        let world = TestWorld;
        let state = PhysicsState {
            position: Vec3::new(0.0, 10.0, 0.0),
            velocity: Vec3::ZERO,
            movement_mode: MovementMode::Walking,
            realm: Realm::Overworld,
            on_ground: false,
        };
        let input = MovementInput::default();

        let result = simulate_physics_step(&world, &state, &input, 0.1);

        // Should have fallen due to gravity
        assert!(result.new_velocity.y < 0.0);
        assert!(result.new_position.y < state.position.y);
    }

    #[test]
    fn test_flying_mode_no_gravity() {
        let world = TestWorld;
        let state = PhysicsState {
            position: Vec3::new(0.0, 10.0, 0.0),
            velocity: Vec3::ZERO,
            movement_mode: MovementMode::Flying,
            realm: Realm::Overworld,
            on_ground: false,
        };
        let input = MovementInput::default();

        let result = simulate_physics_step(&world, &state, &input, 0.1);

        // In flying mode with no input, should stay still
        assert_eq!(result.new_velocity.y, 0.0);
        assert_eq!(result.new_position, state.position);
    }
}
