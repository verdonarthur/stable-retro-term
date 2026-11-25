use retro_terminal::KeyboardModes;
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Copy,
    Paste,
    Bytes(Vec<u8>),
}

pub(crate) fn key(
    key: Key<&str>,
    text: Option<&str>,
    physical: PhysicalKey,
    modifiers: ModifiersState,
    modes: KeyboardModes,
) -> Option<Action> {
    use NamedKey as Named;
    let ctrl = modifiers.control_key();
    let shift = modifiers.shift_key();
    let alt = modifiers.alt_key();
    if modifiers.super_key() {
        return None;
    }
    if ctrl
        && shift
        && !alt
        && let Key::Character(character) = key
    {
        if character.eq_ignore_ascii_case("c") {
            return Some(Action::Copy);
        }
        if character.eq_ignore_ascii_case("v") {
            return Some(Action::Paste);
        }
    }
    if modes.application_keypad
        && modifiers.is_empty()
        && let Some(suffix) = keypad(physical)
    {
        return Some(Action::Bytes(vec![0x1b, b'O', suffix]));
    }
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
    if let Key::Named(named) = key
        && let Some((code, kind)) = special(named)
    {
        let bytes = if modifier > 1 {
            format!("\x1b[{};{modifier}{}", code.unwrap_or(1), kind as char).into_bytes()
        } else if let Some(code) = code {
            format!("\x1b[{code}{}", kind as char).into_bytes()
        } else {
            let application = matches!(named, Named::F1 | Named::F2 | Named::F3 | Named::F4)
                || modes.application_cursor;
            vec![0x1b, if application { b'O' } else { b'[' }, kind]
        };
        return Some(Action::Bytes(bytes));
    }
    let mut bytes = match key {
        Key::Named(Named::Enter) => vec![b'\r'],
        Key::Named(Named::Tab) if shift => return Some(Action::Bytes(b"\x1b[Z".to_vec())),
        Key::Named(Named::Tab) => vec![b'\t'],
        Key::Named(Named::Escape) => vec![0x1b],
        Key::Named(Named::Backspace) => vec![if ctrl { 8 } else { 127 }],
        Key::Named(Named::Space) if ctrl => vec![0],
        Key::Character(character) if ctrl => vec![control_byte(character)?],
        Key::Character(character) => text.unwrap_or(character).as_bytes().to_vec(),
        Key::Named(Named::Space) => vec![b' '],
        _ => return None,
    };
    if alt {
        bytes.insert(0, 0x1b);
    }
    Some(Action::Bytes(bytes))
}

fn control_byte(character: &str) -> Option<u8> {
    let [byte] = *character.as_bytes() else {
        return None;
    };
    match byte.to_ascii_uppercase() {
        b'@'..=b'_' => Some(byte.to_ascii_uppercase() & 0x1f),
        b' ' | b'2' => Some(0),
        b'3'..=b'7' => Some(byte - b'3' + 27),
        b'?' | b'8' => Some(127),
        _ => None,
    }
}

fn keypad(physical: PhysicalKey) -> Option<u8> {
    use KeyCode::*;
    Some(match physical {
        PhysicalKey::Code(Numpad0) => b'p',
        PhysicalKey::Code(Numpad1) => b'q',
        PhysicalKey::Code(Numpad2) => b'r',
        PhysicalKey::Code(Numpad3) => b's',
        PhysicalKey::Code(Numpad4) => b't',
        PhysicalKey::Code(Numpad5) => b'u',
        PhysicalKey::Code(Numpad6) => b'v',
        PhysicalKey::Code(Numpad7) => b'w',
        PhysicalKey::Code(Numpad8) => b'x',
        PhysicalKey::Code(Numpad9) => b'y',
        PhysicalKey::Code(NumpadDecimal) => b'n',
        PhysicalKey::Code(NumpadComma) => b'l',
        PhysicalKey::Code(NumpadAdd) => b'k',
        PhysicalKey::Code(NumpadSubtract) => b'm',
        PhysicalKey::Code(NumpadMultiply) => b'j',
        PhysicalKey::Code(NumpadDivide) => b'o',
        PhysicalKey::Code(NumpadEqual) => b'X',
        PhysicalKey::Code(NumpadEnter) => b'M',
        _ => return None,
    })
}

fn special(key: NamedKey) -> Option<(Option<u8>, u8)> {
    use NamedKey::*;
    Some(match key {
        ArrowUp => (None, b'A'),
        ArrowDown => (None, b'B'),
        ArrowRight => (None, b'C'),
        ArrowLeft => (None, b'D'),
        Home => (None, b'H'),
        End => (None, b'F'),
        F1 => (None, b'P'),
        F2 => (None, b'Q'),
        F3 => (None, b'R'),
        F4 => (None, b'S'),
        Insert => (Some(2), b'~'),
        Delete => (Some(3), b'~'),
        PageUp => (Some(5), b'~'),
        PageDown => (Some(6), b'~'),
        F5 => (Some(15), b'~'),
        F6 => (Some(17), b'~'),
        F7 => (Some(18), b'~'),
        F8 => (Some(19), b'~'),
        F9 => (Some(20), b'~'),
        F10 => (Some(21), b'~'),
        F11 => (Some(23), b'~'),
        F12 => (Some(24), b'~'),
        _ => return None,
    })
}

#[derive(Default)]
pub(crate) struct Composition {
    pub(crate) preedit: String,
}

impl Composition {
    pub(crate) fn update(&mut self, event: winit::event::Ime) -> Option<String> {
        use winit::event::Ime;
        match event {
            Ime::Preedit(text, _) => self.preedit = text,
            Ime::Disabled => self.preedit.clear(),
            Ime::Commit(text) => {
                self.preedit.clear();
                return Some(text);
            }
            Ime::Enabled => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::event::Ime;

    #[test]
    fn shortcuts_take_precedence_over_terminal_control_keys() {
        let physical = PhysicalKey::Code(KeyCode::KeyC);
        let modes = KeyboardModes::default();
        let ctrl = ModifiersState::CONTROL;
        assert_eq!(
            key(Key::Character("c"), None, physical, ctrl, modes),
            Some(Action::Bytes(vec![3]))
        );
        assert_eq!(
            key(
                Key::Character("C"),
                None,
                physical,
                ctrl | ModifiersState::SHIFT,
                modes
            ),
            Some(Action::Copy)
        );
        assert_eq!(
            key(
                Key::Character("V"),
                None,
                PhysicalKey::Code(KeyCode::KeyV),
                ctrl | ModifiersState::SHIFT,
                modes
            ),
            Some(Action::Paste)
        );
    }

    #[test]
    fn applications_can_switch_cursor_and_keypad_modes() {
        let modes = KeyboardModes {
            application_cursor: true,
            application_keypad: true,
        };
        let arrow = Key::Named(NamedKey::ArrowUp);
        let physical = PhysicalKey::Code(KeyCode::ArrowUp);
        assert_eq!(
            key(
                arrow.clone(),
                None,
                physical,
                ModifiersState::empty(),
                KeyboardModes::default()
            ),
            Some(Action::Bytes(b"\x1b[A".to_vec()))
        );
        assert_eq!(
            key(
                arrow.clone(),
                None,
                physical,
                ModifiersState::empty(),
                modes
            ),
            Some(Action::Bytes(b"\x1bOA".to_vec()))
        );
        assert_eq!(
            key(arrow, None, physical, ModifiersState::CONTROL, modes),
            Some(Action::Bytes(b"\x1b[1;5A".to_vec()))
        );
        assert_eq!(
            key(
                Key::Character("3"),
                Some("3"),
                PhysicalKey::Code(KeyCode::Numpad3),
                ModifiersState::empty(),
                modes
            ),
            Some(Action::Bytes(b"\x1bOs".to_vec()))
        );
    }

    #[test]
    fn composition_only_emits_committed_text() {
        let mut composition = Composition::default();
        assert_eq!(
            composition.update(Ime::Preedit("に".into(), Some((0, 3)))),
            None
        );
        assert_eq!(composition.preedit, "に");
        assert_eq!(composition.update(Ime::Preedit(String::new(), None)), None);
        assert_eq!(
            composition.update(Ime::Commit("日".into())),
            Some("日".into())
        );
        assert!(composition.preedit.is_empty());
    }
}
