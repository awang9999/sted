use crate::terminal::{Position, Size, Terminal};
use crate::view::View;
use core::cmp::{max, min};
use crossterm::event::{
    Event::{self, FocusGained, FocusLost, Key, Mouse, Paste, Resize},
    KeyCode,
    KeyCode::Char,
    KeyEvent, KeyModifiers, read,
};

pub struct Editor {
    should_quit: bool,
    position: Position,
    view: View,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            should_quit: false,
            position: Position { x: 0, y: 0 },
            view: View::default(),
        }
    }
}

impl Editor {
    pub fn handle_args(&mut self) {
        let args: Vec<String> = std::env::args().collect();

        if let Some(file_name) = args.get(1) {
            self.view.load(file_name);
        }
    }

    pub fn run(&mut self) {
        Terminal::initialize().unwrap();
        self.handle_args();
        let result = self.repl();
        Terminal::terminate().unwrap();
        result.unwrap();
    }

    fn repl(&mut self) -> Result<(), std::io::Error> {
        loop {
            self.refresh_screen()?;
            if self.should_quit {
                break;
            }
            let event = read()?;
            self.evaluate_event(event)?;
        }
        Ok(())
    }

    fn evaluate_event(&mut self, event: Event) -> Result<(), std::io::Error> {
        match event {
            Key(key_event) => self.process_key_event(key_event),
            Mouse(_mouse_event) => Ok(()),
            FocusGained => Ok(()),
            FocusLost => Ok(()),
            Resize(width, height) => self.process_resize_event(width.into(), height.into()),
            Paste(_pasted_string) => Ok(()),
        }?;

        Ok(())
    }

    fn process_resize_event(&mut self, width: usize, height: usize) -> Result<(), std::io::Error> {
        self.view.resize(Size {
            width: width,
            height: height,
        });
        Ok(())
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> Result<(), std::io::Error> {
        let KeyEvent {
            code, modifiers, ..
        } = key_event;

        match code {
            Char('q') if modifiers == KeyModifiers::CONTROL => {
                self.should_quit = true;
            }
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End => self.move_caret(code),
            _ => (),
        }

        Ok(())
    }

    fn move_caret(&mut self, code: KeyCode) {
        let Size { width, height } = self.view.get_size();

        match code {
            KeyCode::Up => self.position.y = max(0, self.position.y.saturating_sub(1)),
            KeyCode::Down => self.position.y = min(width, self.position.y.saturating_add(1)),
            KeyCode::Left => self.position.x = max(0, self.position.x.saturating_sub(1)),
            KeyCode::Right => self.position.x = min(width, self.position.x.saturating_add(1)),
            KeyCode::PageUp => self.position.y = 0,
            KeyCode::PageDown => self.position.y = height,
            KeyCode::Home => self.position.x = 0,
            KeyCode::End => self.position.x = width,
            _ => (),
        }
    }

    fn refresh_screen(&mut self) -> Result<(), std::io::Error> {
        Terminal::hide_cursor()?;
        if self.should_quit {
            Terminal::clear_screen()?;
            Terminal::move_cursor_to(0, 0)?;
            Terminal::print("Goodbye. \r\n")?;
        } else {
            if self.view.is_modified() {
                self.view.render()?;
            }
            Terminal::move_cursor_to_pos(&self.position)?;
        }
        Terminal::show_cursor()?;
        Terminal::execute()?;
        Ok(())
    }
}
