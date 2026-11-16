// Pure keyboard state machine: the hook does no disk or audio work.
pub const CTRL: u8 = 1;
pub const SHIFT: u8 = 2;
pub const ALT: u8 = 4;
pub const WIN: u8 = 8;

pub fn modifier(vk: u16) -> u8 {
    match vk {
        0x11 | 0xa2 | 0xa3 => CTRL,
        0x10 | 0xa0 | 0xa1 => SHIFT,
        0x12 | 0xa4 | 0xa5 => ALT,
        0x5b | 0x5c => WIN,
        _ => 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub key: u16,
    pub mods: u8,
}

impl Default for Shortcut {
    fn default() -> Self {
        Self { key: 0x11, mods: 0 }
    }
}

impl Shortcut {
    pub fn matches(self, vk: u16) -> bool {
        self.key == vk
            || matches!(
                (self.key, vk),
                (0x11, 0xa2 | 0xa3) | (0x10, 0xa0 | 0xa1) | (0x12, 0xa4 | 0xa5)
            )
    }

    pub fn valid(self) -> bool {
        (1..=254).contains(&self.key) && self.mods & !15 == 0 && self.mods & modifier(self.key) == 0
    }
}

pub struct Keyboard {
    down: [bool; 256],
    armed: bool,
    blocked: bool,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self {
            down: [false; 256],
            armed: false,
            blocked: false,
        }
    }
}

impl Keyboard {
    pub fn mods(&self) -> u8 {
        self.down
            .iter()
            .enumerate()
            .filter(|(_, down)| **down)
            .fold(0, |mask, (vk, _)| mask | modifier(vk as u16))
    }

    pub fn cancel(&mut self) {
        self.armed = false;
        self.blocked = true;
    }

    // Keep physical key state even while settings are open, but never arm a toggle.
    pub fn observe(&mut self, vk: u16, pressed: bool) {
        if vk < 256 {
            self.down[vk as usize] = pressed;
        }
        self.armed = false;
        self.blocked = false;
    }

    pub fn event(&mut self, shortcut: Shortcut, vk: u16, pressed: bool) -> bool {
        if vk >= 256 {
            return false;
        }
        let repeat = self.down[vk as usize] == pressed;
        self.down[vk as usize] = pressed;
        if repeat {
            return false;
        }
        if pressed {
            if shortcut.matches(vk) {
                // A modifier by itself must be tapped without another held key.
                let clean = modifier(shortcut.key) == 0
                    || !self.down.iter().enumerate().any(|(key, down)| {
                        *down && !shortcut.matches(key as u16) && modifier(key as u16) == 0
                    });
                self.armed = clean && (self.mods() & !modifier(shortcut.key)) == shortcut.mods;
                self.blocked = false;
            } else if self.armed {
                self.blocked = true;
            }
            false
        } else if shortcut.matches(vk) {
            let fire = self.armed && !self.blocked;
            self.armed = false;
            self.blocked = false;
            fire
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ctrl_taps_toggle_once_and_autorepeat_does_not() {
        let mut k = Keyboard::default();
        let s = Shortcut::default();
        assert!(!k.event(s, 0xa2, true));
        assert!(!k.event(s, 0xa2, true));
        assert!(k.event(s, 0xa2, false));
        assert!(!k.event(s, 0xa2, false));
        assert!(!k.event(s, 0xa3, true));
        assert!(k.event(s, 0xa3, false));
    }
    #[test]
    fn ctrl_copy_and_ctrl_mouse_do_not_toggle() {
        let mut k = Keyboard::default();
        let s = Shortcut::default();
        k.event(s, 0xa2, true);
        k.event(s, 0x43, true);
        k.event(s, 0x43, false);
        assert!(!k.event(s, 0xa2, false));
        k.event(s, 0xa2, true);
        k.cancel();
        assert!(!k.event(s, 0xa2, false));
    }
    #[test]
    fn existing_held_key_and_extra_modifiers_do_not_toggle() {
        let mut k = Keyboard::default();
        let s = Shortcut::default();
        k.event(s, 0x41, true);
        k.event(s, 0xa2, true);
        assert!(!k.event(s, 0xa2, false));
        k.event(s, 0x41, false);
        k.event(s, 0xa0, true);
        k.event(s, 0xa2, true);
        assert!(!k.event(s, 0xa2, false));
    }
    #[test]
    fn combo_fires_on_release_even_if_modifier_released_first() {
        let mut k = Keyboard::default();
        let s = Shortcut {
            key: 0x4d,
            mods: CTRL | SHIFT,
        };
        k.event(s, 0xa2, true);
        k.event(s, 0xa0, true);
        assert!(!k.event(s, 0x4d, true));
        assert!(!k.event(s, 0x4d, true));
        k.event(s, 0xa2, false);
        k.event(s, 0xa0, false);
        assert!(k.event(s, 0x4d, false));
    }
    #[test]
    fn unrelated_keys_cancel_and_settings_release_cannot_trigger() {
        let mut k = Keyboard::default();
        let s = Shortcut { key: 0x77, mods: 0 };
        k.event(s, 0x77, true);
        k.event(s, 0x41, true);
        assert!(!k.event(s, 0x77, false));
        k.observe(0x77, true);
        assert!(!k.event(s, 0x77, false));
    }
}
