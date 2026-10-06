#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
}

impl Button {
    pub const fn mask(self) -> u8 {
        1 << self as u8
    }

    pub const fn from_value(value: i64) -> Option<Self> {
        match value {
            1 => Some(Self::Up),
            2 => Some(Self::Down),
            4 => Some(Self::Left),
            8 => Some(Self::Right),
            16 => Some(Self::A),
            32 => Some(Self::B),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct InputState {
    current: u8,
    sampled: u8,
    pending_pressed: u8,
    pressed: u8,
}

impl InputState {
    pub fn set_button_mask(&mut self, current: u8) {
        self.sample_buttons(current);
        self.begin_tick();
    }

    /// Retain presses even when a complete down/up tap occurs between ticks.
    pub fn sample_buttons(&mut self, current: u8) {
        let current = current & 0b00_111111;
        self.pending_pressed |= current & !self.sampled;
        self.sampled = current;
    }

    pub fn begin_tick(&mut self) {
        self.current = self.sampled;
        self.pressed = self.pending_pressed;
        self.pending_pressed = 0;
    }

    pub fn finish_tick(&mut self) {
        self.pressed = 0;
    }

    pub const fn is_down(self, button: Button) -> bool {
        self.current & button.mask() != 0
    }

    pub const fn was_just_pressed(self, button: Button) -> bool {
        self.pressed & button.mask() != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressed_is_true_only_on_the_transition_frame() {
        let mut input = InputState::default();
        let a = Button::A;

        input.set_button_mask(a.mask());
        assert!(input.is_down(a));
        assert!(input.was_just_pressed(a));

        input.set_button_mask(a.mask());
        assert!(input.is_down(a));
        assert!(!input.was_just_pressed(a));

        input.set_button_mask(0);
        assert!(!input.is_down(a));
        assert!(!input.was_just_pressed(a));

        input.set_button_mask(a.mask());
        assert!(input.was_just_pressed(a));
    }
}
