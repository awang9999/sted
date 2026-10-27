use crate::commandbar::{CommandBar, CommandBarOutcome};
use crate::common::constants::LONG_MESSAGE_DURATION;
use crate::common::types::Location;
use crate::editorcommand::{Direction, EditorCommand};
use crate::terminal::Position;
use crate::view::View;

/// The prompt text of the save-as prompt.
const SAVE_AS_PROMPT: &str = "Save as: ";
/// The prompt text of the search prompt.
const SEARCH_PROMPT: &str = "Search (Esc to cancel, Arrows to navigate): ";
/// Shown on the command bar right of the typed pattern while a search
/// matches nothing in the buffer.
const NO_MATCHES: &str = "(No matches)";

/// What a prompt open on the command bar is asking for.
///
/// Adding a new input prompt (open file, …) means adding a variant here
/// and its arms in [`PromptPurpose::config`],
/// [`PromptPurpose::closes_on_confirmation`], and [`PromptPurpose::apply`]
/// — the command bar and the editor stay untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptPurpose {
    /// Collects the file name to save the current document to.
    SaveAs,
    /// Collects the pattern to search for in the buffer.
    Find,
}

impl PromptPurpose {
    /// The prompt text and (optional) pre-filled text of the prompt.
    fn config(self) -> (&'static str, Option<&'static str>) {
        match self {
            // A new document has no file name to pre-fill with.
            PromptPurpose::SaveAs => (SAVE_AS_PROMPT, None),
            // No previous search pattern to pre-fill with yet.
            PromptPurpose::Find => (SEARCH_PROMPT, None),
        }
    }

    /// Whether confirming the prompt closes it: both purposes act on
    /// their entered text, so a confirmation always closes the prompt. A
    /// purpose that keeps the prompt open on Enter would return false
    /// here.
    fn closes_on_confirmation(self) -> bool {
        match self {
            PromptPurpose::SaveAs | PromptPurpose::Find => true,
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
            // Leaving the search on Enter puts the caret on the match, so
            // the user can continue editing there; a pattern without a
            // match leaves the caret where it is. Either way the prompt
            // closed, so the view stops highlighting the match.
            (PromptPurpose::Find, PromptResolution::Confirmed(pattern)) => {
                view.find_first(&pattern);
                view.clear_live_search();
            }
            (PromptPurpose::Find, PromptResolution::Cancelled) => {
                view.set_message(String::from("Search aborted"), LONG_MESSAGE_DURATION);
                view.clear_live_search();
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
    /// The caret position the buffer had when a find prompt opened, so
    /// dismissing the search can restore it.
    find_origin: Option<Location>,
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
        // A find prompt moves the caret while it is open; remember where
        // it was so dismissing the search can restore it.
        let find_origin = (purpose == PromptPurpose::Find).then_some(view.location);
        Self {
            bar,
            purpose,
            find_origin,
        }
    }

    /// Routes a command to the prompt's bar and reports how the prompt
    /// closed, or `None` while it stays open.
    ///
    /// While a find prompt is open, every change to the typed pattern
    /// re-runs the live search: the caret tracks the first match from
    /// where the search began onwards, wrapping to the top of the buffer
    /// when nothing matches after it, and the view highlights the match,
    /// centered in the viewport, while the caret returns to where the
    /// search began when the pattern matches nothing in the buffer. The
    /// arrow keys do not move the caret in the pattern but navigate
    /// between matches instead: right and down find the next match after
    /// the current one, left and up the previous one, both wrapping
    /// around the buffer. Any other caret movement inside the prompt
    /// keeps the current match.
    pub fn handle(&mut self, view: &mut View, command: EditorCommand) -> Option<PromptResolution> {
        // The viewport resizes together with the prompt.
        if let EditorCommand::Resize(size) = command {
            view.resize(size);
        }

        // While the find prompt is open, the arrow keys navigate between
        // matches instead of moving the caret in the typed pattern.
        if self.purpose == PromptPurpose::Find {
            match command {
                EditorCommand::Move(Direction::Right | Direction::Down) => {
                    view.search_next();
                    return None;
                }
                EditorCommand::Move(Direction::Left | Direction::Up) => {
                    view.search_prev();
                    return None;
                }
                _ => {}
            }
        }

        // The live search re-runs only when the typed pattern changes.
        let pattern_before = self.bar.get_text();

        match self.bar.handle_command(command) {
            CommandBarOutcome::Stay => {
                if self.purpose == PromptPurpose::Find
                    && let Some(origin) = self.find_origin
                    && self.bar.get_text() != pattern_before
                {
                    // A pattern without a match puts the caret back where
                    // the search began, so it never lingers on a word the
                    // pattern only partially matches.
                    view.update_live_search(&self.bar.get_text(), origin);
                }
                None
            }
            CommandBarOutcome::Confirmed(text) => {
                // The purpose decides whether a confirmation closes the
                // prompt.
                if self.purpose.closes_on_confirmation() {
                    Some(PromptResolution::Confirmed(text))
                } else {
                    None
                }
            }
            CommandBarOutcome::Cancelled => Some(PromptResolution::Cancelled),
        }
    }

    /// Applies the purpose-specific reaction to a closed prompt.
    ///
    /// Dismissing a find prompt also restores the caret position the
    /// buffer had when the search opened and scrolls it into view.
    pub fn resolve(&self, view: &mut View, resolution: PromptResolution) {
        // `apply` consumes the resolution, so detect the dismissal before
        // reacting to it.
        let dismissed = matches!(resolution, PromptResolution::Cancelled);
        self.purpose.apply(view, resolution);
        if dismissed && let Some(origin) = self.find_origin {
            view.set_location_and_scroll(origin);
        }
    }

    /// Renders the prompt's bar and returns the on-screen position of its
    /// caret.
    ///
    /// While a find prompt is open and its pattern matches nothing, the
    /// bar shows "(No matches)" right of the typed text; an empty pattern
    /// is no search yet, so it shows nothing.
    pub fn render(&mut self, view: &View) -> Position {
        self.bar.render(self.no_match_note(view));
        self.bar.get_caret_position()
    }

    /// The note the bar shows right of the typed text: `NO_MATCHES` while
    /// a non-empty find pattern matches nothing in the buffer.
    fn no_match_note(&self, view: &View) -> Option<&'static str> {
        (self.purpose == PromptPurpose::Find
            && !self.bar.is_empty()
            && !view.live_search_has_match())
        .then_some(NO_MATCHES)
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
    fn find_prompt_starts_empty_with_the_search_prompt_text() {
        let view = view();
        let session = PromptSession::open(&view, PromptPurpose::Find);
        assert!(session.bar().is_empty());
        // The caret sits right behind the
        // "Search (Esc to cancel, Arrows to navigate): " prompt.
        assert_eq!(session.bar().get_caret_position().col, SEARCH_PROMPT.len());
        assert_eq!(session.bar().get_caret_position().row, 23);
    }

    #[test]
    fn handle_closes_the_search_prompt_on_enter_with_the_typed_pattern() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        // The search starts at the top of the buffer.
        view.set_location(0, 0);
        let mut session = PromptSession::open(&view, PromptPurpose::Find);
        session.handle(&mut view, EditorCommand::Insert('b'));
        // The live search already moved the caret to the match while the
        // prompt stayed open.
        assert_eq!((view.location.x, view.location.y), (1, 0));
        assert_eq!(
            session.handle(&mut view, EditorCommand::Enter),
            Some(PromptResolution::Confirmed(String::from("b")))
        );
    }

    #[test]
    fn handle_of_the_search_prompt_tracks_the_first_match_while_typing() {
        // The first "foo " is followed by "cat", so only once "b" is typed
        // does the first match jump to the second "foo ".
        let mut view = view();
        for c in "foo cat foo bar".chars() {
            view.handle_command(EditorCommand::Insert(c));
        }
        // The search starts at the top of the buffer.
        view.set_location(0, 0);
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        session.handle(&mut view, EditorCommand::Insert('f'));
        assert_eq!((view.location.x, view.location.y), (0, 0));
        session.handle(&mut view, EditorCommand::Insert('o'));
        assert_eq!((view.location.x, view.location.y), (0, 0));
        session.handle(&mut view, EditorCommand::Insert('o'));
        assert_eq!((view.location.x, view.location.y), (0, 0));
        session.handle(&mut view, EditorCommand::Insert(' '));
        assert_eq!((view.location.x, view.location.y), (0, 0));
        session.handle(&mut view, EditorCommand::Insert('b'));
        assert_eq!((view.location.x, view.location.y), (8, 0));
    }

    #[test]
    fn the_search_starts_from_the_caret_position_not_the_top() {
        let mut view = view();
        for c in "foo cat foo bar".chars() {
            view.handle_command(EditorCommand::Insert(c));
        }
        // The caret sits on the second "foo".
        view.set_location(8, 0);
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        // The first "foo" is before the caret, so "f" matches the second one.
        session.handle(&mut view, EditorCommand::Insert('f'));
        assert_eq!((view.location.x, view.location.y), (8, 0));
        for c in "oo b".chars() {
            session.handle(&mut view, EditorCommand::Insert(c));
        }
        // "foo b" occurs only after the caret.
        assert_eq!((view.location.x, view.location.y), (8, 0));
    }

    #[test]
    fn handle_of_the_search_prompt_without_a_match_keeps_the_caret_at_the_search_origin() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        view.set_location(1, 0);
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        session.handle(&mut view, EditorCommand::Insert('z'));

        assert_eq!((view.location.x, view.location.y), (1, 0));
    }

    #[test]
    fn handle_of_the_search_prompt_with_the_pattern_erased_keeps_the_caret_at_the_search_origin() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        view.set_location(1, 0);
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        session.handle(&mut view, EditorCommand::Insert('z'));
        // Erasing the pattern back to empty matches nothing, so the caret
        // stays at the search origin.
        session.handle(&mut view, EditorCommand::BackSpace);

        assert_eq!((view.location.x, view.location.y), (1, 0));
    }

    #[test]
    fn a_pattern_that_stops_matching_returns_the_caret_to_the_search_origin() {
        let mut view = view();
        for c in "file is file".chars() {
            view.handle_command(EditorCommand::Insert(c));
        }
        // The search starts with the caret after the first "file".
        view.set_location(4, 0);
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        for c in "fil".chars() {
            session.handle(&mut view, EditorCommand::Insert(c));
        }
        // Each prefix of "filo" matches the second "file".
        assert_eq!((view.location.x, view.location.y), (8, 0));

        // "filo" matches nothing: the caret returns where the search
        // began instead of sitting on "file".
        session.handle(&mut view, EditorCommand::Insert('o'));
        assert_eq!((view.location.x, view.location.y), (4, 0));

        // Backspacing to a matching pattern finds the match again.
        session.handle(&mut view, EditorCommand::BackSpace);
        assert_eq!((view.location.x, view.location.y), (8, 0));
    }

    #[test]
    fn the_live_search_updates_with_the_typed_pattern() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        session.handle(&mut view, EditorCommand::Insert('b'));
        assert_eq!(view.live_search_pattern(), Some("b"));
        session.handle(&mut view, EditorCommand::BackSpace);
        // Erasing the pattern back to empty clears the match it found.
        assert_eq!(view.live_search_pattern(), Some(""));
    }

    #[test]
    fn caret_movement_in_the_search_prompt_keeps_the_live_search() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        view.set_location(0, 0);
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        session.handle(&mut view, EditorCommand::Insert('b'));
        // Home moves the caret in the pattern, not between matches.
        session.handle(&mut view, EditorCommand::Move(Direction::Home));

        // The pattern did not change, so neither did the match.
        assert_eq!(view.live_search_pattern(), Some("b"));
        assert_eq!((view.location.x, view.location.y), (1, 0));
    }

    #[test]
    fn right_and_down_find_the_next_match_while_the_search_prompt_is_open() {
        let mut view = view();
        for c in "foo cat foo bar".chars() {
            view.handle_command(EditorCommand::Insert(c));
        }
        view.set_location(0, 0);
        let mut session = PromptSession::open(&view, PromptPurpose::Find);
        session.handle(&mut view, EditorCommand::Insert('f'));
        assert_eq!((view.location.x, view.location.y), (0, 0));

        session.handle(&mut view, EditorCommand::Move(Direction::Right));
        assert_eq!((view.location.x, view.location.y), (8, 0));
        // The arrow moved the match, not the caret in the pattern.
        assert_eq!(
            session.bar().get_caret_position().col,
            SEARCH_PROMPT.len() + 1
        );
        session.handle(&mut view, EditorCommand::Move(Direction::Down));
        // Only two "f" matches: the next step wraps to the first.
        assert_eq!((view.location.x, view.location.y), (0, 0));
    }

    #[test]
    fn left_and_up_find_the_previous_match_while_the_search_prompt_is_open() {
        let mut view = view();
        for c in "foo cat foo bar".chars() {
            view.handle_command(EditorCommand::Insert(c));
        }
        // The caret sits on the second "foo".
        view.set_location(8, 0);
        let mut session = PromptSession::open(&view, PromptPurpose::Find);
        session.handle(&mut view, EditorCommand::Insert('f'));
        // The search starts at the caret: the match is the second "foo".
        assert_eq!((view.location.x, view.location.y), (8, 0));

        session.handle(&mut view, EditorCommand::Move(Direction::Left));
        assert_eq!((view.location.x, view.location.y), (0, 0));
        session.handle(&mut view, EditorCommand::Move(Direction::Up));
        // Only two "f" matches: the next step wraps to the last one.
        assert_eq!((view.location.x, view.location.y), (8, 0));
    }

    #[test]
    fn arrows_without_a_match_do_not_move_the_caret() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        session.handle(&mut view, EditorCommand::Insert('z'));
        session.handle(&mut view, EditorCommand::Move(Direction::Right));
        session.handle(&mut view, EditorCommand::Move(Direction::Left));

        assert_eq!((view.location.x, view.location.y), (2, 0));
    }

    #[test]
    fn arrows_still_move_the_caret_in_the_save_prompt() {
        let mut view = view();
        let mut session = PromptSession::open(&view, PromptPurpose::SaveAs);

        session.handle(&mut view, EditorCommand::Insert('a'));
        session.handle(&mut view, EditorCommand::Move(Direction::Left));

        assert_eq!(session.bar().get_caret_position().col, SAVE_AS_PROMPT.len());
    }

    #[test]
    fn closing_the_search_prompt_clears_the_live_search() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('b'));
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        session.handle(&mut view, EditorCommand::Insert('b'));
        let resolution = session.handle(&mut view, EditorCommand::Enter);
        session.resolve(&mut view, resolution.expect("Enter closes the prompt"));

        assert_eq!(view.live_search_pattern(), None);
    }

    #[test]
    fn dismissing_the_search_prompt_clears_the_live_search() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('b'));
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        session.handle(&mut view, EditorCommand::Insert('b'));
        let resolution = session.handle(&mut view, EditorCommand::Escape);
        session.resolve(&mut view, resolution.expect("Esc closes the prompt"));

        assert_eq!(view.live_search_pattern(), None);
    }

    #[test]
    fn the_save_prompt_does_not_start_a_live_search() {
        let mut view = view();
        let mut session = PromptSession::open(&view, PromptPurpose::SaveAs);

        session.handle(&mut view, EditorCommand::Insert('a'));

        assert_eq!(view.live_search_pattern(), None);
    }

    #[test]
    fn a_find_pattern_without_a_match_gets_the_no_matches_note() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        session.handle(&mut view, EditorCommand::Insert('z'));

        assert_eq!(session.no_match_note(&view), Some(NO_MATCHES));
    }

    #[test]
    fn a_find_pattern_that_wraps_to_a_match_before_the_origin_has_no_note() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        // The caret sits at the end of the buffer.
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        // "b" matches only before the caret, but the search wraps to it.
        session.handle(&mut view, EditorCommand::Insert('b'));

        assert_eq!((view.location.x, view.location.y), (1, 0));
        assert_eq!(session.no_match_note(&view), None);
    }

    #[test]
    fn a_matching_find_pattern_has_no_note() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        view.set_location(0, 0);
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        session.handle(&mut view, EditorCommand::Insert('b'));

        assert_eq!(session.no_match_note(&view), None);
    }

    #[test]
    fn the_no_matches_note_follows_the_pattern_live() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        view.set_location(0, 0);
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        session.handle(&mut view, EditorCommand::Insert('z'));
        assert_eq!(session.no_match_note(&view), Some(NO_MATCHES));
        // Backspacing to an empty pattern is no search yet: no note.
        session.handle(&mut view, EditorCommand::BackSpace);
        assert_eq!(session.no_match_note(&view), None);
        // A matching pattern has no note either.
        session.handle(&mut view, EditorCommand::Insert('b'));
        assert_eq!(session.no_match_note(&view), None);
        // The note comes back when the pattern stops matching.
        session.handle(&mut view, EditorCommand::Insert('z'));
        assert_eq!(session.no_match_note(&view), Some(NO_MATCHES));
    }

    #[test]
    fn an_empty_find_pattern_has_no_note() {
        let view = view();
        let session = PromptSession::open(&view, PromptPurpose::Find);

        assert_eq!(session.no_match_note(&view), None);
    }

    #[test]
    fn the_save_prompt_has_no_note() {
        let mut view = view();
        let mut session = PromptSession::open(&view, PromptPurpose::SaveAs);

        session.handle(&mut view, EditorCommand::Insert('a'));

        assert_eq!(session.no_match_note(&view), None);
    }

    #[test]
    fn resolve_confirmation_of_the_search_prompt_leaves_the_caret_on_the_match() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        view.set_location(0, 0);
        let session = PromptSession::open(&view, PromptPurpose::Find);

        session.resolve(&mut view, PromptResolution::Confirmed(String::from("b")));

        assert_eq!((view.location.x, view.location.y), (1, 0));
    }

    #[test]
    fn resolve_cancellation_of_the_search_prompt_restores_the_caret() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        let session = PromptSession::open(&view, PromptPurpose::Find);

        session.resolve(&mut view, PromptResolution::Cancelled);

        // The caret was at the end of the text when the search opened.
        assert_eq!((view.location.x, view.location.y), (2, 0));
    }

    #[test]
    fn dismissing_a_search_restores_the_caret_the_search_moved() {
        let mut view = view();
        view.handle_command(EditorCommand::Insert('a'));
        view.handle_command(EditorCommand::Insert('b'));
        view.set_location(0, 0);
        let mut session = PromptSession::open(&view, PromptPurpose::Find);

        // "b" matches after the caret, so the search moves it.
        session.handle(&mut view, EditorCommand::Insert('b'));
        assert_eq!((view.location.x, view.location.y), (1, 0));
        let resolution = session.handle(&mut view, EditorCommand::Escape);
        assert_eq!(resolution, Some(PromptResolution::Cancelled));
        session.resolve(&mut view, PromptResolution::Cancelled);

        // The caret is back where the search started.
        assert_eq!((view.location.x, view.location.y), (0, 0));
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

    #[test]
    fn resolve_cancellation_of_the_search_prompt_reports_on_the_message_bar() {
        let mut view = view();
        let session = PromptSession::open(&view, PromptPurpose::Find);

        session.resolve(&mut view, PromptResolution::Cancelled);

        let message = view.get_message();
        assert_eq!(message.message, "Search aborted");
        assert_eq!(message.duration, LONG_MESSAGE_DURATION);
    }
}
