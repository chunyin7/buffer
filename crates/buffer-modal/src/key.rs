/// A key press as keymaps see it: a key and the modifiers held with it.
///
/// Keys are logical: a char key holds the char the keyboard layout typed, so Shift is already in
/// it, and Shift+a is `A`. [`Key::new`] drops Shift from char keys, so a press matches the same
/// binding whether or not Shift is reported with it. Other keys keep it: Shift+Tab is not Tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    code: KeyCode,
    modifiers: Modifiers,
}

impl Key {
    /// `code` with `modifiers`, less Shift if `code` is a char.
    pub fn new(code: KeyCode, modifiers: Modifiers) -> Self {
        let shift = modifiers.shift && !matches!(code, KeyCode::Char(_));
        Self {
            code,
            modifiers: Modifiers { shift, ..modifiers },
        }
    }

    /// The key that types `c`, without modifiers.
    pub fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c), Modifiers::NONE)
    }

    pub fn code(&self) -> KeyCode {
        self.code
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }
}

/// A key by what it means in the keyboard layout, not where it sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// A key that types a char. Space is `Char(' ')`.
    Char(char),
    Esc,
    Enter,
    Tab,
    Backspace,
    Delete,
    Insert,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

/// The modifier keys held with a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Command on macOS, the Windows key on Windows, Super on Linux.
    pub super_key: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        super_key: false,
    };
    pub const CTRL: Self = Self {
        ctrl: true,
        ..Self::NONE
    };
    pub const ALT: Self = Self {
        alt: true,
        ..Self::NONE
    };
    pub const SHIFT: Self = Self {
        shift: true,
        ..Self::NONE
    };
    pub const SUPER: Self = Self {
        super_key: true,
        ..Self::NONE
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_is_part_of_a_char_but_not_of_other_keys() {
        // Shift+a arrives as `A`, with or without Shift reported.
        let a = KeyCode::Char('A');
        assert_eq!(Key::new(a, Modifiers::SHIFT), Key::char('A'));
        let ctrl_shift = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        };
        assert_eq!(Key::new(a, ctrl_shift), Key::new(a, Modifiers::CTRL));
        assert_ne!(
            Key::new(KeyCode::Tab, Modifiers::SHIFT),
            Key::new(KeyCode::Tab, Modifiers::NONE)
        );
    }
}
