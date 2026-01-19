use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

use crate::messages::{ActionMask, TransmittableAction};

/// Player input actions expressed for leafwing + lightyear pipelines.
/// We keep the surface area aligned with the existing `TransmittableAction`
/// bits so current movement/block interactions can be driven from a single
/// source of truth.
#[derive(Actionlike, Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect)]
pub enum PlayerInputAction {
    Move,
    Jump,
    Crouch,
    ToggleFly,
    Hit,
    Modify,
}

impl PlayerInputAction {
    pub fn default_input_map() -> InputMap<PlayerInputAction> {
        InputMap::default()
            // Movement on WASD + left stick
            .with_dual_axis(Self::Move, VirtualDPad::wasd())
            .with_dual_axis(Self::Move, GamepadStick::LEFT)
            // Jump / crouch
            .with(Self::Jump, KeyCode::Space)
            .with(Self::Jump, GamepadButton::South)
            .with(Self::Crouch, KeyCode::ShiftLeft)
            .with(Self::Crouch, GamepadButton::East)
            // Toggle fly
            .with(Self::ToggleFly, KeyCode::F1)
            .with(Self::ToggleFly, GamepadButton::West)
            // Interactions
            .with(Self::Hit, MouseButton::Left)
            .with(Self::Modify, MouseButton::Right)
    }
}

/// Convert a leafwing action state to our existing `ActionMask` bitfield.
/// This lets the current gameplay logic keep operating while we migrate
/// transport to lightyear.
pub fn action_mask_from_leafwing(state: &ActionState<PlayerInputAction>) -> ActionMask {
    const AXIS_DEADZONE: f32 = 0.15;

    let mut mask = ActionMask::default();

    let axis = state.axis_pair(&PlayerInputAction::Move);
    if axis.y > AXIS_DEADZONE {
        mask.insert(TransmittableAction::MoveForward);
    } else if axis.y < -AXIS_DEADZONE {
        mask.insert(TransmittableAction::MoveBackward);
    }
    if axis.x > AXIS_DEADZONE {
        mask.insert(TransmittableAction::MoveRight);
    } else if axis.x < -AXIS_DEADZONE {
        mask.insert(TransmittableAction::MoveLeft);
    }

    if state.pressed(&PlayerInputAction::Jump) {
        mask.insert(TransmittableAction::JumpOrFlyUp);
    }
    if state.pressed(&PlayerInputAction::Crouch) {
        mask.insert(TransmittableAction::CrouchOrFlyDown);
    }
    if state.just_pressed(&PlayerInputAction::ToggleFly) {
        mask.insert(TransmittableAction::ToggleFlyMode);
    }
    if state.pressed(&PlayerInputAction::Hit) {
        mask.insert(TransmittableAction::Hit);
    }
    if state.pressed(&PlayerInputAction::Modify) {
        mask.insert(TransmittableAction::Modify);
    }

    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_axes_set_bits() {
        let mut state = ActionState::<PlayerInputAction>::default();
        state.set_axis_pair(&PlayerInputAction::Move, Vec2::new(1.0, 0.8));
        let mask = action_mask_from_leafwing(&state);
        assert!(mask.contains(TransmittableAction::MoveForward));
        assert!(mask.contains(TransmittableAction::MoveRight));
    }

    #[test]
    fn toggle_fly_is_edge_triggered() {
        let mut state = ActionState::<PlayerInputAction>::default();
        state.press(&PlayerInputAction::ToggleFly);
        let mask = action_mask_from_leafwing(&state);
        assert!(mask.contains(TransmittableAction::ToggleFlyMode));
    }
}
