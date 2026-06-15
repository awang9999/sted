use crate::common::types::Size;
use crate::terminal::Terminal;
use crate::view::{CaretDirection, View};
use crossterm::event::{
    Event::{self, FocusGained, FocusLost, Key, Mouse, Paste, Resize},
    KeyCode,
    KeyCode::Char,
    KeyEvent, KeyModifiers, read,
};

pub struct Editor {
    should_quit: bool,
    view: View,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            should_quit: false,
            view: View::default(),
        }
    }
}

impl Editor {
    pub fn new() -> Result<Self, std::io::Error> {
        let current_hook = std::panic::take_hook();

        std::panic::set_hook(Box::new(move |panic_info| {
            //do the cleanup work
            let _ = Terminal::terminate();
            current_hook(panic_info);
        }));

        Terminal::initialize()?;

        let mut view = View::default();

        let args: Vec<String> = std::env::args().collect();

        if let Some(file_name) = args.get(1) {
            let _ = view.load(file_name);
        }

        Ok(Self {
            should_quit: false,
            view,
        })
    }

    pub fn run(&mut self) {
        loop {
            self.refresh_screen();
            if self.should_quit {
                break;
            }

            match read() {
                Ok(event) => self.evaluate_event(event),
                Err(err) => {
                    #[cfg(debug_assertions)]
                    {
                        panic!("Could not read event: {err:?}");
                    }
                }
            }
        }
    }

    fn evaluate_event(&mut self, event: Event) {
        match event {
            Key(key_event) => self.process_key_event(key_event),
            Mouse(_mouse_event) => (),
            FocusGained => (),
            FocusLost => (),
            Resize(width, height) => self.process_resize_event(width.into(), height.into()),
            Paste(_pasted_string) => (),
        };
    }

    fn process_resize_event(&mut self, width: usize, height: usize) {
        self.view.resize(Size { width, height });
    }

    fn process_key_event(&mut self, key_event: KeyEvent) {
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
    }

    fn move_caret(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up => self.view.move_caret_position(CaretDirection::Up),
            KeyCode::Down => self.view.move_caret_position(CaretDirection::Down),
            KeyCode::Left => self.view.move_caret_position(CaretDirection::Left),
            KeyCode::Right => self.view.move_caret_position(CaretDirection::Right),
            KeyCode::PageUp => self.view.move_caret_position(CaretDirection::Top),
            KeyCode::PageDown => self.view.move_caret_position(CaretDirection::Bottom),
            KeyCode::Home => self.view.move_caret_position(CaretDirection::LineStart),
            KeyCode::End => self.view.move_caret_position(CaretDirection::LineEnd),
            _ => (),
        }
    }

    fn refresh_screen(&mut self) {
        let _ = Terminal::hide_cursor();
        self.view.render();
        let _ = Terminal::move_cursor_to_pos(&self.view.caret_position);
        let _ = Terminal::show_cursor();
        let _ = Terminal::execute();
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        let _ = Terminal::terminate();
        if self.should_quit {
            let _ = Terminal::print("Goodbye. \r\n");
        }
    }
}
