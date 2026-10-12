use crate::common::constants::{DEFAULT_MESSAGE_DURATION, EVENT_POLL_INTERVAL};
use crate::editorcommand::EditorCommand;
use crate::terminal::Terminal;
use crate::view::View;
use crossterm::event::{
    Event::{self, Key, Resize},
    poll, read,
};

pub struct Editor {
    should_quit: bool,
    view: View,
}

impl Default for Editor {
    fn default() -> Self {
        let terminal_size = Terminal::size().unwrap_or_default();
        Self {
            should_quit: false,
            view: View::new(terminal_size),
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

        let terminal_size = Terminal::size().unwrap_or_default();
        let mut view = View::new(terminal_size);

        let args: Vec<String> = std::env::args().collect();

        if let Some(file_name) = args.get(1) {
            view.load(file_name);
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

            // Poll instead of blocking on read() so time-based updates —
            // like message bar expiry — fire while the user is idle.
            match poll(EVENT_POLL_INTERVAL) {
                Ok(true) => match read() {
                    Ok(event) => self.evaluate_event(event),
                    Err(err) => Self::report_event_error(&err),
                },
                Ok(false) => {}
                Err(err) => Self::report_event_error(&err),
            }
        }
    }

    /// Event I/O errors are reported in debug builds; in release they are
    /// intentionally ignored and the loop continues.
    fn report_event_error(err: &std::io::Error) {
        #[cfg(debug_assertions)]
        {
            panic!("Could not read event: {err:?}");
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = err;
        }
    }

    fn evaluate_event(&mut self, event: Event) {
        let should_process = matches!(event, Key(_) | Resize(_, _));

        if should_process {
            match EditorCommand::try_from(event) {
                Ok(command) => {
                    if matches!(command, EditorCommand::Quit) {
                        self.should_quit = true;
                    } else {
                        self.view.handle_command(command);
                    }
                }
                Err(err) => {
                    // Report the unrecognized command on the message bar; it
                    // stays visible for the default duration.
                    self.view.set_message(err, DEFAULT_MESSAGE_DURATION);
                }
            }
        } else {
            #[cfg(debug_assertions)]
            {
                panic!("Received and discarded unsupported or non-press event.");
            }
        }
    }

    fn refresh_screen(&mut self) {
        let _ = Terminal::hide_cursor();
        self.view.render();
        let _ = Terminal::move_cursor_to_pos(&self.view.get_caret_position());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::time::Duration;

    #[test]
    fn unrecognized_command_is_reported_on_the_message_bar() {
        let mut editor = Editor::default();

        let event = Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        editor.evaluate_event(event);

        let message = editor.view.get_message();
        assert_eq!(message.message, "Key Code not supported: F(1)");
        assert_eq!(
            message.duration,
            Duration::from_secs(DEFAULT_MESSAGE_DURATION)
        );
    }
}
