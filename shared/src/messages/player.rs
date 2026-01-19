use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Eq, Hash)]
#[repr(u8)]
pub enum TransmittableAction {
    MoveForward,
    MoveBackward,
    MoveLeft,
    MoveRight,
    JumpOrFlyUp,
    CrouchOrFlyDown,
    ToggleFlyMode,
    Hit,
    Modify,
}

/// Bitflag representation of player actions for compact storage and fast checks.
#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, Clone, Copy)]
pub struct ActionMask(pub u16);

impl ActionMask {
    #[inline]
    pub fn insert(&mut self, action: TransmittableAction) {
        self.0 |= 1 << (action as u16);
    }

    #[inline]
    pub fn contains(&self, action: TransmittableAction) -> bool {
        (self.0 & (1 << (action as u16))) != 0
    }
}
