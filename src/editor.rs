use crate::common::constants::{DEFAULT_MESSAGE_DURATION, EVENT_POLL_INTERVAL};
use crate::editorcommand::EditorCommand;
use crate::prompt::{PromptPurpose, PromptSession};
use crate::terminal::Terminal;
use crate::view::View;
use crossterm::event::{
    Event::{self, Key, Resize},
    poll, read,
};

pub struct Editor {
    should_quit: bool,
    view: View,
    /// Open whenever the editor needs a single line of input from the
    /// user. While it is open, the command bar replaces the message bar
    /// and the prompt is modal: no command reaches the view.
    prompt: Option<PromptSession>,
}

impl Default for Editor {
    fn default() -> Self {
        let terminal_size = Terminal::size().unwrap_or_default();
        Self {
            should_quit: false,
            view: View::new(terminal_size),
            prompt: None,
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
            prompt: None,
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
            let command = match EditorCommand::try_from(event) {
                Ok(command) => command,
                Err(err) => {
                    // While a prompt is up, it is modal and unknown keys
                    // are swallowed instead of being reported on the
                    // (hidden) message bar.
                    if self.prompt.is_none() {
                        self.view.set_message(err, DEFAULT_MESSAGE_DURATION);
                    }
                    return;
                }
            };

            if self.prompt.is_some() {
                self.handle_prompt_command(command);
                return;
            }

            match command {
                EditorCommand::Quit => {
                    // The view may refuse the first quit request when the
                    // buffer has unsaved changes, warning on the message
                    // bar that a second Ctrl-Q confirms the quit.
                    if self.view.handle_quit() {
                        self.should_quit = true;
                    }
                }
                EditorCommand::Save => {
                    if self.view.has_file_name() {
                        self.view.handle_command(EditorCommand::Save);
                    } else {
                        // A new document has no file to save to: ask the
                        // user for a file name on the command bar.
                        self.open_prompt(PromptPurpose::SaveAs);
                    }
                }
                _ => self.view.handle_command(command),
            }
        } else {
            #[cfg(debug_assertions)]
            {
                panic!("Received and discarded unsupported or non-press event.");
            }
        }
    }

    /// Opens a prompt on the command bar's row, which takes over the
    /// message bar's row while it is open.
    fn open_prompt(&mut self, purpose: PromptPurpose) {
        self.prompt = Some(PromptSession::open(&self.view, purpose));
    }

    /// Routes a command to the active prompt and applies the outcome of a
    /// closed prompt to the view.
    fn handle_prompt_command(&mut self, command: EditorCommand) {
        let mut session = self.prompt.take().expect("the prompt is open");
        if let Some(resolution) = session.handle(&mut self.view, command) {
            session.resolve(&mut self.view, resolution);
            // The prompt closed, so `self.prompt` stays `None`.
        } else {
            self.prompt = Some(session);
        }
    }

    fn refresh_screen(&mut self) {
        let _ = Terminal::hide_cursor();
        self.view.render();
        // While a prompt is active it replaces the message bar, so it
        // draws after the view (which may still redraw the message bar)
        // and it owns the terminal cursor.
        let caret_position = match &mut self.prompt {
            Some(session) => session.render(),
            None => self.view.get_caret_position(),
        };
        let _ = Terminal::move_cursor_to_pos(&caret_position);
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
    use crate::common::constants::LONG_MESSAGE_DURATION;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn unrecognized_command_is_reported_on_the_message_bar() {
        let mut editor = Editor::default();

        let event = Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        editor.evaluate_event(event);

        let message = editor.view.get_message();
        assert_eq!(message.message, "Key Code not supported: F(1)");
        assert_eq!(message.duration, DEFAULT_MESSAGE_DURATION);
    }

    fn ctrl_q() -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL))
    }

    fn ctrl_s() -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
    }

    fn key(c: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn temp_path(name: &str) -> String {
        std::env::temp_dir()
            .join(format!("{name}_{}.txt", std::process::id()))
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn quit_with_unsaved_changes_requires_a_second_press() {
        let mut editor = Editor::default();
        let path =
            std::env::temp_dir().join(format!("sted_editor_quit_{}.txt", std::process::id()));
        std::fs::write(&path, "hello\n").expect("create temp file");
        let file_path = path.to_string_lossy().to_string();
        editor.view.load(&file_path);

        // Make the buffer modified so quitting needs confirmation.
        editor.view.handle_command(EditorCommand::Insert('x'));

        // First Ctrl-Q: warned, not quitting yet.
        editor.evaluate_event(ctrl_q());
        assert!(!editor.should_quit);
        assert_eq!(
            editor.view.get_message().message,
            format!(
                "WARNING! File {file_path} has unsaved changes. Press Ctrl-Q again to quit without saving"
            )
        );

        // Second Ctrl-Q: quits.
        editor.evaluate_event(ctrl_q());
        assert!(editor.should_quit);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn quit_without_unsaved_changes_exits_on_first_press() {
        let mut editor = Editor::default();
        let path =
            std::env::temp_dir().join(format!("sted_editor_quit_clean_{}.txt", std::process::id()));
        std::fs::write(&path, "hello\n").expect("create temp file");
        let file_path = path.to_string_lossy().to_string();
        editor.view.load(&file_path);

        editor.evaluate_event(ctrl_q());

        assert!(editor.should_quit);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_without_file_name_opens_the_command_bar() {
        let mut editor = Editor::default();
        let message_before = editor.view.get_message();

        editor.evaluate_event(ctrl_s());

        assert!(editor.prompt.is_some());
        // Opening the prompt must not touch the buffer or the message bar.
        assert_eq!(editor.view.get_status().total_lines, 0);
        assert_eq!(editor.view.get_message(), message_before);
    }

    #[test]
    fn save_with_file_name_does_not_open_the_command_bar() {
        let mut editor = Editor::default();
        let path = temp_path("sted_editor_save_named");
        std::fs::write(&path, "hello\n").expect("create temp file");
        editor.view.load(&path);

        editor.evaluate_event(ctrl_s());

        assert!(editor.prompt.is_none());
        assert_eq!(
            editor.view.get_message().message,
            format!("File saved successfully to {path}")
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn characters_typed_into_the_command_bar_do_not_reach_the_buffer() {
        let mut editor = Editor::default();

        editor.evaluate_event(ctrl_s());
        editor.evaluate_event(key('x'));

        assert_eq!(
            editor
                .prompt
                .as_ref()
                .expect("the prompt is open")
                .bar()
                .get_text(),
            "x"
        );
        assert_eq!(editor.view.get_status().total_lines, 0);
    }

    #[test]
    fn enter_with_a_file_name_saves_to_it_and_closes_the_command_bar() {
        let mut editor = Editor::default();
        editor.view.handle_command(EditorCommand::Insert('h'));
        editor.evaluate_event(ctrl_s());

        let path = temp_path("sted_editor_saveas");
        for c in path.chars() {
            editor.evaluate_event(key(c));
        }
        editor.evaluate_event(press(KeyCode::Enter));

        assert!(editor.prompt.is_none());
        assert_eq!(
            editor.view.get_message().message,
            format!("File saved successfully to {path}")
        );
        assert_eq!(std::fs::read_to_string(&path).expect("saved file"), "h\n");
        assert!(!editor.view.get_status().is_modified);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn enter_without_a_file_name_keeps_the_command_bar_open() {
        let mut editor = Editor::default();
        let message_before = editor.view.get_message();

        editor.evaluate_event(ctrl_s());
        editor.evaluate_event(press(KeyCode::Enter));

        assert!(editor.prompt.is_some());
        assert_eq!(editor.view.get_message(), message_before);
    }

    #[test]
    fn escape_closes_the_command_bar_and_reports_save_aborted() {
        let mut editor = Editor::default();

        editor.evaluate_event(ctrl_s());
        editor.evaluate_event(press(KeyCode::Esc));

        assert!(editor.prompt.is_none());
        let message = editor.view.get_message();
        assert_eq!(message.message, "Save aborted");
        assert_eq!(message.duration, LONG_MESSAGE_DURATION);
    }

    #[test]
    fn unknown_keys_are_swallowed_while_the_command_bar_is_open() {
        let mut editor = Editor::default();
        let message_before = editor.view.get_message();

        editor.evaluate_event(ctrl_s());
        editor.evaluate_event(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)));

        assert!(editor.prompt.is_some());
        assert_eq!(editor.view.get_message(), message_before);
    }

    #[test]
    fn ctrl_q_aborts_the_command_bar_without_quitting() {
        let mut editor = Editor::default();

        editor.evaluate_event(ctrl_s());
        editor.evaluate_event(ctrl_q());

        // The prompt is dismissed (and the buffer left untouched) but the
        // editor itself must not quit on the first Ctrl-Q.
        assert!(editor.prompt.is_none());
        assert!(!editor.should_quit);
        assert_eq!(editor.view.get_message().message, "Save aborted");
    }

    #[test]
    fn resize_while_the_prompt_is_open_reaches_the_command_bar() {
        let mut editor = Editor::default();

        editor.evaluate_event(ctrl_s());
        editor.evaluate_event(Event::Resize(100, 30));

        let bar = editor
            .prompt
            .as_ref()
            .expect("the prompt is open")
            .bar()
            .get_caret_position();
        assert_eq!(bar.row, 29);
    }
}
