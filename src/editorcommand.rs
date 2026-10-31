use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use super::terminal::Size;

#[derive(Clone, Copy)]
pub enum Direction {
    PageUp,
    PageDown,
    Home,
    End,
    Up,
    Left,
    Right,
    Down,
}

#[derive(Clone, Copy)]
pub enum EditorCommand {
    Move(Direction),
    Resize(Size),
    Insert(char),
    Enter,
    Delete,
    BackSpace,
    Quit,
    Save,
    Find,
    Escape,
}

impl TryFrom<Event> for EditorCommand {
    type Error = String;

    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match event {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (code, modifiers) {
                // Program control
                (KeyCode::Char('q'), KeyModifiers::CONTROL) => Ok(Self::Quit),
                (KeyCode::Char('s'), KeyModifiers::CONTROL) => Ok(Self::Save),
                (KeyCode::Char('f'), KeyModifiers::CONTROL) => Ok(Self::Find),
                (KeyCode::Esc, KeyModifiers::NONE) => Ok(Self::Escape),
                // Caret movement
                (KeyCode::Up, KeyModifiers::NONE) => Ok(Self::Move(Direction::Up)),
                (KeyCode::Down, KeyModifiers::NONE) => Ok(Self::Move(Direction::Down)),
                (KeyCode::Left, KeyModifiers::NONE) => Ok(Self::Move(Direction::Left)),
                (KeyCode::Right, KeyModifiers::NONE) => Ok(Self::Move(Direction::Right)),
                (KeyCode::PageDown, KeyModifiers::NONE) => Ok(Self::Move(Direction::PageDown)),
                (KeyCode::PageUp, KeyModifiers::NONE) => Ok(Self::Move(Direction::PageUp)),
                (KeyCode::Home, KeyModifiers::NONE) => Ok(Self::Move(Direction::Home)),
                (KeyCode::End, KeyModifiers::NONE) => Ok(Self::Move(Direction::End)),
                // Ordinary presses
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => Ok(Self::Insert(c)),
                // Tab inserts a literal tab character
                (KeyCode::Tab, KeyModifiers::NONE) => Ok(Self::Insert('\t')),
                // Enter
                (KeyCode::Enter, KeyModifiers::NONE) => Ok(Self::Enter),
                // Delete
                (KeyCode::Delete, KeyModifiers::NONE) => Ok(Self::Delete),
                // Backspace
                (KeyCode::Backspace, KeyModifiers::NONE) => Ok(Self::BackSpace),
                _ => Err(format!("Key Code not supported: {code:?}")),
            },
            Event::Resize(width_u16, height_u16) => {
                // `u16` always fits in `usize`, so this conversion is exact
                // on every Rust target.
                let height = usize::from(height_u16);
                let width = usize::from(width_u16);
                Ok(Self::Resize(Size { height, width }))
            }
            _ => Err(format!("Event not supported: {event:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn tab_maps_to_inserting_a_tab_character() {
        let command =
            EditorCommand::try_from(press(KeyCode::Tab)).expect("Tab should be supported");
        assert!(matches!(command, EditorCommand::Insert(c) if c == '\t'));
    }

    #[test]
    fn enter_maps_to_enter() {
        let command =
            EditorCommand::try_from(press(KeyCode::Enter)).expect("Enter should be supported");
        assert!(matches!(command, EditorCommand::Enter));
    }

    #[test]
    fn plain_character_maps_to_insert() {
        let command =
            EditorCommand::try_from(press(KeyCode::Char('a'))).expect("Char should be supported");
        assert!(matches!(command, EditorCommand::Insert('a')));
    }

    #[test]
    fn unsupported_key_is_an_error() {
        let result = EditorCommand::try_from(press(KeyCode::F(1)));
        assert!(result.is_err());
    }

    #[test]
    fn ctrl_s_maps_to_save() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        let command = EditorCommand::try_from(event).expect("Ctrl-S should be supported");
        assert!(matches!(command, EditorCommand::Save));
    }

    #[test]
    fn ctrl_f_maps_to_find() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        let command = EditorCommand::try_from(event).expect("Ctrl-F should be supported");
        assert!(matches!(command, EditorCommand::Find));
    }

    #[test]
    fn esc_maps_to_escape() {
        let command =
            EditorCommand::try_from(press(KeyCode::Esc)).expect("Esc should be supported");
        assert!(matches!(command, EditorCommand::Escape));
    }
}
