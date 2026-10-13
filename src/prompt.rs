use crate::commandbar::{CommandBar, CommandBarOutcome};
use crate::common::constants::LONG_MESSAGE_DURATION;
use crate::editorcommand::EditorCommand;
use crate::terminal::Position;
use crate::view::View;

/// The prompt text of the save-as prompt.
const SAVE_AS_PROMPT: &str = "Save as: ";

/// What a prompt open on the command bar is asking for.
///
/// Adding a new input prompt (open file, search, …) means adding a
/// variant here and its arms in [`PromptPurpose::config`] and
/// [`PromptPurpose::apply`] — the command bar and the editor stay
/// untouched.
#[derive(Debug, Clone, Copy)]
pub enum PromptPurpose {
    /// Collects the file name to save the current document to.
    SaveAs,
}

impl PromptPurpose {
    /// The prompt text and (optional) pre-filled text of the prompt.
    fn config(self) -> (&'static str, Option<&'static str>) {
        match self {
            // A new document has no file name to pre-fill with.
            PromptPurpose::SaveAs => (SAVE_AS_PROMPT, None),
        }
    }

    /// Reacts to a closed prompt: a confirmation acts on the entered text
    /// according to the purpose, a cancellation reports on the message
    /// bar.
    fn apply(self, view: &mut View, resolution: PromptResolution) {
        match (self, resolution) {
            (PromptPurpose::SaveAs, PromptResolution::Confirmed(file_name)) => {
                view.save_as(&file_name);
            }
            (PromptPurpose::SaveAs, PromptResolution::Cancelled) => {
                view.set_message(String::from("Save aborted"), LONG_MESSAGE_DURATION);
            }
        }
    }
}

/// How a prompt closed.
#[derive(Debug, PartialEq)]
pub enum PromptResolution {
    /// The user confirmed the prompt (Enter) with the entered text.
    Confirmed(String),
    /// The user dismissed the prompt (Esc or quit) without confirming.
    Cancelled,
}

/// A prompt open on the command bar: the input bar itself and the purpose
/// the entered text is collected for.
///
/// A prompt is modal: while it is open, the command bar replaces the
/// message bar and no command reaches the view.
pub struct PromptSession {
    bar: CommandBar,
    purpose: PromptPurpose,
}

impl PromptSession {
    /// Opens a prompt on the command bar's row, which takes over the
    /// message bar's row while it is open.
    pub fn open(view: &View, purpose: PromptPurpose) -> Self {
        let (prompt_text, initial_text) = purpose.config();
        let mut bar = CommandBar::new(view.terminal_size(), prompt_text);
        if let Some(initial_text) = initial_text {
            bar = bar.with_initial_text(initial_text);
        }
        Self { bar, purpose }
    }

    /// Routes a command to the prompt's bar and reports how the prompt
    /// closed, or `None` while it stays open.
    pub fn handle(&mut self, view: &mut View, command: EditorCommand) -> Option<PromptResolution> {
        // The viewport resizes together with the prompt.
        if let EditorCommand::Resize(size) = command {
            view.resize(size);
        }

        match self.bar.handle_command(command) {
            CommandBarOutcome::Stay => None,
            CommandBarOutcome::Confirmed(text) => Some(PromptResolution::Confirmed(text)),
            CommandBarOutcome::Cancelled => Some(PromptResolution::Cancelled),
        }
    }

    /// Applies the purpose-specific reaction to a closed prompt.
    pub fn resolve(&self, view: &mut View, resolution: PromptResolution) {
        self.purpose.apply(view, resolution);
    }

    /// Renders the prompt's bar and returns the on-screen position of its
    /// caret.
    pub fn render(&mut self) -> Position {
        self.bar.render();
        self.bar.get_caret_position()
    }

    /// The input bar behind the prompt.
    // Only the test suite inspects the bar directly; the editor renders
    // it through [`PromptSession::render`].
    #[allow(dead_code)]
    pub fn bar(&self) -> &CommandBar {
        &self.bar
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Size;

    fn view() -> View {
        View::new(Size {
            width: 80,
            height: 24,
        })
    }

    #[test]
    fn open_starts_empty_on_the_message_bar_row() {
        let view = view();
        let session = PromptSession::open(&view, PromptPurpose::SaveAs);
        assert!(session.bar().is_empty());
        assert_eq!(session.bar().get_caret_position().row, 23);
    }

    #[test]
    fn handle_stays_open_for_editing_commands() {
        let mut view = view();
        let mut session = PromptSession::open(&view, PromptPurpose::SaveAs);
        assert!(
            session
                .handle(&mut view, EditorCommand::Insert('a'))
                .is_none()
        );
        assert_eq!(session.bar().get_text(), "a");
    }

    #[test]
    fn handle_reports_confirmation_with_the_entered_text() {
        let mut view = view();
        let mut session = PromptSession::open(&view, PromptPurpose::SaveAs);
        session.handle(&mut view, EditorCommand::Insert('a'));
        assert_eq!(
            session.handle(&mut view, EditorCommand::Enter),
            Some(PromptResolution::Confirmed(String::from("a")))
        );
    }

    #[test]
    fn handle_reports_cancellation_on_escape() {
        let mut view = view();
        let mut session = PromptSession::open(&view, PromptPurpose::SaveAs);
        assert_eq!(
            session.handle(&mut view, EditorCommand::Escape),
            Some(PromptResolution::Cancelled)
        );
    }

    #[test]
    fn handle_resizes_the_view_together_with_the_bar() {
        let mut view = view();
        let mut session = PromptSession::open(&view, PromptPurpose::SaveAs);
        session.handle(
            &mut view,
            EditorCommand::Resize(Size {
                width: 100,
                height: 30,
            }),
        );
        assert_eq!(session.bar().get_caret_position().row, 29);
    }

    #[test]
    fn resolve_confirmation_saves_to_the_entered_file() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('h'));
        let path = std::env::temp_dir()
            .join(format!("sted_prompt_save_{}.txt", std::process::id()))
            .to_string_lossy()
            .to_string();

        let session = PromptSession::open(&view, PromptPurpose::SaveAs);
        session.resolve(&mut view, PromptResolution::Confirmed(path.clone()));

        assert!(std::path::Path::new(&path).exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn resolve_cancellation_reports_on_the_message_bar() {
        let mut view = view();
        let session = PromptSession::open(&view, PromptPurpose::SaveAs);

        session.resolve(&mut view, PromptResolution::Cancelled);

        let message = view.get_message();
        assert_eq!(message.message, "Save aborted");
        assert_eq!(message.duration, LONG_MESSAGE_DURATION);
    }
}
