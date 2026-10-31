use crate::buffer::Buffer;
use crate::common::constants::{
    DEFAULT_MESSAGE_DURATION, LONG_MESSAGE_DURATION, RESERVED_BOTTOM_LINES,
};
use crate::common::line::{Line, SegmentHighlight};
use crate::common::types::Location;
use crate::editorcommand::{Direction, EditorCommand};
use crate::messagebar::{MessageBar, MessageState};
use crate::statusbar::{DocumentStatus, StatusBar};
use crate::terminal::{Position, Size, Terminal};
use std::ops::Range;
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;

/// Shown on the message bar until a more specific message replaces it.
const DEFAULT_MESSAGE: &str = "HELP: Ctrl-F = find | Ctrl-S = save | Ctrl-Q = quit";

pub struct View {
    buffer: Buffer,
    /// True when the buffer viewport has changed since the last render and
    /// must be drawn again. Distinct from `Buffer.modified`, which tracks
    /// unsaved changes.
    needs_redraw: bool,
    size: Size,
    pub location: Location,      // location in the text (col, row) of grapheme
    pub scroll_offset: Position, // top left corner of viewport as a grid (independent of grapheme width)
    desired_x: usize,
    status_bar: StatusBar,
    message_bar: MessageBar,
    /// Set when a quit request was refused because of unsaved changes; a
    /// second consecutive Ctrl-Q then confirms the quit.
    quit_pending: bool,
    /// The pattern the open search prompt is searching for, if any. While
    /// set, the buffer highlights every visible occurrence of the pattern —
    /// and the occurrence the caret sits on stands out — so the search
    /// results are visible while the prompt is still open.
    search_pattern: Option<String>,
}

impl View {
    pub fn new(size: Size) -> Self {
        let mut message_bar = MessageBar::new(size);
        message_bar.set_message(String::from(DEFAULT_MESSAGE), DEFAULT_MESSAGE_DURATION);
        View {
            buffer: Buffer::default(),
            needs_redraw: true,
            size: Self::viewport_size(size),
            location: Location::default(),
            scroll_offset: Position::default(),
            desired_x: 0,
            status_bar: StatusBar::new(size),
            message_bar,
            quit_pending: false,
            search_pattern: None,
        }
    }

    /// The part of the terminal the buffer is rendered into: the full
    /// terminal minus the rows reserved for the status bar at the bottom.
    fn viewport_size(terminal: Size) -> Size {
        Size {
            width: terminal.width,
            height: terminal.height.saturating_sub(RESERVED_BOTTOM_LINES),
        }
    }

    pub fn get_status(&self) -> DocumentStatus {
        DocumentStatus {
            file_name: self.buffer.file_name.clone(),
            total_lines: self.buffer.lines.len(),
            current_line: self.location.y,
            is_modified: self.buffer.modified,
        }
    }

    /// Whether the buffer is associated with a file on disk. A buffer
    /// without a file name is a new, unsaved document.
    pub fn has_file_name(&self) -> bool {
        self.buffer.file_name.is_some()
    }

    /// The view's current message, read from the message bar that owns
    /// it (state and display timer). Returning a clone is what keeps the
    /// message from being re-armed on every render pass.
    // Only the test suite reads the message so far; the editor reports
    // messages without inspecting the previous one.
    #[allow(dead_code)]
    pub fn get_message(&self) -> MessageState {
        self.message_bar.message().clone()
    }

    /// Replaces the message shown on the message bar and (re)starts its
    /// display timer, so it stays visible for `duration`.
    pub fn set_message(&mut self, message: String, duration: Duration) {
        self.message_bar.set_message(message, duration);
    }

    /// The full terminal size: the viewport plus the rows reserved for
    /// the bars at the bottom.
    pub fn terminal_size(&self) -> Size {
        Size {
            width: self.size.width,
            height: self.size.height.saturating_add(RESERVED_BOTTOM_LINES),
        }
    }

    pub fn resize(&mut self, to: Size) {
        // When the terminal shrinks, every row at or above the new height is
        // gone, but the new last row keeps whatever it held before (buffer
        // text or the status bar). Clear it so no stale content lingers below
        // the viewport, since neither render pass touches that row.
        let old_terminal_height = self.size.height.saturating_add(RESERVED_BOTTOM_LINES);
        if to.height > 0 && to.height < old_terminal_height {
            let _ = Terminal::clear_row(to.height.saturating_sub(1));
        }
        self.size = Self::viewport_size(to);
        self.needs_redraw = true;
        self.status_bar.resize(to);
        self.message_bar.resize(to);
    }

    pub fn load(&mut self, file_name: &str) {
        match Buffer::load(file_name) {
            Ok(buffer) => self.buffer = buffer,
            Err(error) => {
                // The file could not be opened: fall back to an empty buffer
                // (so nothing fabricated can ever be saved to disk) and keep
                // the error visible until the user has read it.
                self.buffer = Buffer::default();
                self.set_message(error, LONG_MESSAGE_DURATION);
            }
        }
        self.needs_redraw = true;
    }

    pub fn get_caret_position(&self) -> Position {
        let caret_position = Position {
            col: self.buffer.get_col_from_text_location(self.location),
            row: self.location.y,
        };
        caret_position.subtract(&self.scroll_offset)
    }

    pub fn handle_command(&mut self, event: EditorCommand) {
        // Any other activity supersedes a pending quit warning, so the user
        // must press Ctrl-Q twice again to quit after it.
        self.quit_pending = false;
        match event {
            EditorCommand::Resize(size) => self.resize(size),
            EditorCommand::Move(direction) => self.move_location(direction),
            EditorCommand::Insert(c) => self.handle_insert(c),
            EditorCommand::Enter => self.handle_newline(),
            EditorCommand::Delete => self.handle_delete(),
            EditorCommand::BackSpace => self.handle_backspace(),
            EditorCommand::Save => self.handle_save(),
            // Quit and Find are handled by the editor. Escape is only
            // meaningful to the command bar. The view ignores all three.
            EditorCommand::Quit | EditorCommand::Find | EditorCommand::Escape => (),
        }
    }

    /// Handles a quit request and reports whether the editor should exit.
    ///
    /// When the buffer has unsaved changes, the first request is refused and
    /// a warning is shown on the message bar; a second consecutive request
    /// confirms the quit. Any other command in between (see
    /// `handle_command`) resets the warning.
    pub fn handle_quit(&mut self) -> bool {
        if self.buffer.modified && !self.quit_pending {
            self.quit_pending = true;
            let file_name = self.buffer.file_name.as_deref().unwrap_or("(unnamed)");
            self.set_message(
                format!("WARNING! File {file_name} has unsaved changes. Press Ctrl-Q again to quit without saving"),
                LONG_MESSAGE_DURATION,
            );
            return false;
        }
        true
    }

    pub fn render_buffer(&mut self) {
        let Size { height, width } = self.size;
        if height == 0 || width == 0 {
            return;
        }

        // The open live search, if any: every visible occurrence of the
        // pattern is drawn in the match highlight, and the occurrence the
        // caret sits on stands out in the active-match highlight. While no
        // search is open — or its pattern is still empty — nothing is
        // highlighted.
        let active = self.live_search_highlight();

        for current_row in 0..height {
            let _ = Terminal::clear_row(current_row);
            let top = self.scroll_offset.row;
            let buffer_row = current_row.saturating_add(top);

            if let Some(line) = self.buffer.lines.get(buffer_row) {
                let left = self.scroll_offset.col;
                let right = self.scroll_offset.col.saturating_add(width);
                let matches = self.line_match_spans(line);
                let row_active = active
                    .as_ref()
                    .filter(|(row, _)| *row == buffer_row)
                    .map(|(_, span)| span);
                Self::render_buffer_row(
                    current_row,
                    line,
                    &(left..right),
                    matches.as_deref(),
                    row_active,
                );
            } else {
                Self::render_line(current_row, "~");
            }

            if current_row.saturating_add(1) < height {
                let _ = Terminal::move_cursor_to(0, current_row.saturating_add(1));
            }
        }
    }

    /// Draws one buffer row: the line's visible portion as display
    /// segments, each drawn according to its highlight — plain, in the
    /// match highlight where a span of `matches` covers it, and in the
    /// stronger active-match highlight where `active` covers it.
    fn render_buffer_row(
        row: usize,
        line: &Line,
        range: &Range<usize>,
        matches: Option<&[Range<usize>]>,
        active: Option<&Range<usize>>,
    ) {
        // `clear_row` keeps the cursor's column; the segments must start
        // at the row's beginning.
        let _ = Terminal::move_cursor_to(0, row);
        for (text, highlight) in line.display_segments(range, matches, active) {
            match highlight {
                SegmentHighlight::None => {
                    let _ = Terminal::print(&text);
                }
                SegmentHighlight::Match => {
                    let _ = Terminal::print_match(&text);
                }
                SegmentHighlight::Active => {
                    let _ = Terminal::print_active_match(&text);
                }
            }
        }
    }

    fn render_line(row: usize, line_text: &str) {
        let result = Terminal::print_row(row, line_text);
        debug_assert!(result.is_ok(), "Failed to render line");
    }

    /// Draws everything the view owns: the buffer viewport and the status
    /// bar. The height guards use the live terminal size because the stored
    /// `size` is the viewport size, which cannot distinguish terminal
    /// heights below the reserved bottom rows.
    pub fn render(&mut self) {
        let terminal_height = Terminal::size().unwrap().height;

        // The buffer needs one viewport row: a terminal taller than the
        // reserved bottom rows.
        if terminal_height > RESERVED_BOTTOM_LINES && self.needs_redraw {
            self.render_buffer();

            if self.buffer.is_empty() {
                let _ = self.render_welcome_screen();
            }

            self.needs_redraw = false;
        }

        // The bar always draws from the view's current state, so it can
        // never lag behind the buffer or the caret.
        if terminal_height >= RESERVED_BOTTOM_LINES {
            self.status_bar.update(self.get_status());
            self.status_bar.render();
        }

        if terminal_height >= 1 {
            // The bar owns its message and drops it once it has expired.
            self.message_bar.render();
        }
    }

    fn render_welcome_screen(&self) -> Result<(), std::io::Error> {
        const LINES: [&str; 3] = [
            "Welcome to STED!",
            "The (S)imple (T)erminal (Ed)itor",
            "Version 1.0.0",
        ];

        #[allow(clippy::integer_division)]
        let top_row = (self.size.height / 2).saturating_sub(2);
        // We don't care if the welcome message lands _exactly_ in the
        // middle: it's allowed to sit a bit to the left or right.
        for (offset, text) in LINES.into_iter().enumerate() {
            let row = top_row.saturating_add(offset);
            #[allow(clippy::integer_division)]
            let col = self.size.width.saturating_sub(text.len()) / 2;
            Terminal::print_at(col.saturating_sub(1), row, text)?;
        }
        Ok(())
    }

    pub fn move_location(&mut self, direction: Direction) {
        let Location { mut x, mut y } = self.location;
        let height = self.size.height;
        let buffer_length = self.buffer.lines.len();

        match direction {
            // Vertical movements; Down and PageDown clamp at the "one past
            // the end" row, which is the furthest row we allow navigating to.
            Direction::Up => y = y.saturating_sub(1),
            Direction::Down => y = y.saturating_add(1).min(buffer_length),
            Direction::PageUp => y = y.saturating_sub(height),
            Direction::PageDown => y = y.saturating_add(height).min(buffer_length),

            // Horizontal movements
            Direction::Left => {
                if x == 0 && y > 0 {
                    y = y.saturating_sub(1);
                    x = self.buffer.lines.get(y).map_or(0, Line::grapheme_count);
                } else {
                    x = x.saturating_sub(1);
                }
                self.desired_x = x;
            }

            Direction::Right => {
                if let Some(len) = self.buffer.lines.get(y).map(Line::grapheme_count) {
                    if x >= len && y < buffer_length {
                        y = y.saturating_add(1);
                        x = 0;
                    } else if x < len {
                        x = x.saturating_add(1);
                    }
                }
                self.desired_x = x;
            }

            // Home / End
            Direction::Home => {
                x = 0;
                self.desired_x = x;
            }
            Direction::End => {
                if let Some(len) = self.buffer.lines.get(y).map(Line::grapheme_count) {
                    x = len;
                    self.desired_x = x;
                }
            }
        }

        // Clamp x column to fit within the new line's length when moving vertically
        if matches!(
            direction,
            Direction::Up | Direction::Down | Direction::PageUp | Direction::PageDown
        ) {
            x = self
                .buffer
                .lines
                .get(y)
                .map_or(0, |l| self.desired_x.min(l.grapheme_count()));
        }

        self.set_location(x, y);
        self.scroll_location_into_view();
    }

    pub fn scroll_location_into_view(&mut self) {
        let (x, y) = (self.location.x, self.location.y);
        let mut scroll = self.scroll_offset;
        let Size { width, height } = self.size;
        let prev = self.scroll_offset;

        // Vertical scroll
        // If cursor y is above the viewport, snap scroll_y to the cursor
        if y < scroll.row {
            scroll.row = y;
        }
        // If cursor y is below the viewport, push the viewport down by the offset
        else if y >= scroll.row.saturating_add(height) {
            scroll.row = y.saturating_sub(height).saturating_add(1);
        }

        // Horizontal scroll - when the cursor is on the "one past the end"
        // row, there is no line to take the horizontal scroll from, and
        // `clamped_y` lands on the last line instead.
        let clamped_y = y.min(self.buffer.lines.len().saturating_sub(1));
        let line = self.buffer.lines.get(clamped_y);

        if let Some(line) = line {
            // Use the target line's saved horizontal offset as the baseline
            let line_scroll_x = line.scroll_x;
            let line_graphemes = line.grapheme_count();
            let line_len = line.width_until(line_graphemes);
            let cur_x_position = line.width_until(x);

            // If cursor x is left of the baseline, scroll left to the cursor
            if cur_x_position < line_scroll_x {
                scroll.col = cur_x_position;
            }
            // If cursor x is right of the viewport, scroll right to keep it visible
            else if cur_x_position >= line_scroll_x.saturating_add(width) {
                scroll.col = cur_x_position.saturating_sub(width).saturating_add(1);
            // Use the saved line scroll to keep the cursor relative to the last time the
            // user was on this line.
            } else {
                // Only if x is in the viewport of line_scroll_x but not within the viewport of scroll.x
                // This prevents jumpy experience for a lot of tiny skips while moving vertically
                scroll.col = line_scroll_x;
            }

            // Clamp scroll_x to the line's length so we never scroll past the visible content
            let max_scroll_x = line_len.saturating_sub(width);
            scroll.col = scroll.col.min(max_scroll_x);

            // Persist the updated horizontal scroll state to the target line
            if let Some(line) = self.buffer.lines.get_mut(clamped_y) {
                line.scroll_x = scroll.col;
            }
        }

        // Apply the new scroll offset. A changed scroll needs a
        // re-render, but a pending one from an earlier buffer change
        // must not be cleared: re-rendering is the only way that change
        // becomes visible.
        self.scroll_offset = scroll;
        self.needs_redraw = self.needs_redraw || scroll.col != prev.col || scroll.row != prev.row;
    }

    pub fn set_location(&mut self, x: usize, y: usize) {
        self.location = Location { x, y };
    }

    /// Moves the caret to the first occurrence of `pattern` at or after
    /// the current caret position, scanning forward through the buffer;
    /// when nothing matches after the caret, the search wraps to the top
    /// of the buffer, so a match is found whenever the buffer contains
    /// the pattern at all. Centers the match in the viewport so it is
    /// clearly visible.
    ///
    /// Returns whether a match was found; a pattern the buffer does not
    /// contain anywhere — including the empty one — leaves the caret
    /// where it is.
    pub fn find_first(&mut self, pattern: &str) -> bool {
        match self.buffer.find_wrapping(pattern, self.location) {
            Some(location) => {
                self.set_location_and_center(location);
                true
            }
            None => false,
        }
    }

    /// Moves the caret to `location` and scrolls it into view.
    ///
    /// The movement targets an absolute position (a search match, or the
    /// caret position restored after a dismissed search) rather than a
    /// direction, so the remembered column follows the target column.
    pub fn set_location_and_scroll(&mut self, location: Location) {
        self.set_location(location.x, location.y);
        self.desired_x = location.x;
        self.scroll_location_into_view();
    }

    /// Moves the caret to `location` and scrolls the viewport so the
    /// location sits at its center, making a search match clearly
    /// visible instead of landing at the edge of the viewport.
    pub fn set_location_and_center(&mut self, location: Location) {
        self.set_location(location.x, location.y);
        self.desired_x = location.x;
        self.center_location();
    }

    /// Scrolls the viewport to the center of the caret: its row at the
    /// middle of the viewport and its column at the middle of the
    /// viewport's width, each clamped so the viewport never scrolls above
    /// the start of the document, below its last line, or past the end of
    /// the caret's line.
    fn center_location(&mut self) {
        let (x, y) = (self.location.x, self.location.y);
        let Size { width, height } = self.size;
        let mut scroll = self.scroll_offset;

        // Vertical: the caret's row at the middle of the viewport,
        // clamped to the last row the document scrolls to.
        let target_row = y.saturating_sub(height.saturating_div(2));
        let last_row = self.buffer.lines.len().saturating_sub(height);
        scroll.row = target_row.min(last_row);

        // Horizontal: the caret's column at the middle of the viewport's
        // width, clamped to the line's length.
        let clamped_y = y.min(self.buffer.lines.len().saturating_sub(1));
        if let Some(line) = self.buffer.lines.get(clamped_y) {
            let line_len = line.width_until(line.grapheme_count());
            let target_col = line.width_until(x).saturating_sub(width.saturating_div(2));
            scroll.col = target_col.min(line_len.saturating_sub(width));
            // Persist the new horizontal scroll to the line, as the
            // into-view scrolling does.
            if let Some(line) = self.buffer.lines.get_mut(clamped_y) {
                line.scroll_x = scroll.col;
            }
        }

        self.scroll_offset = scroll;
        self.needs_redraw = true;
    }

    /// Updates the live search shown in the buffer while the search prompt
    /// is open: stores `pattern` so its first match at or after `origin`,
    /// wrapping to the top of the buffer when nothing matches after
    /// `origin`, is highlighted on the next render and the caret moves to
    /// it, centered in the viewport; the caret returns to `origin` when
    /// the pattern matches nothing in the buffer.
    pub fn update_live_search(&mut self, pattern: &str, origin: Location) {
        self.search_pattern = Some(pattern.to_string());
        match self.buffer.find_wrapping(pattern, origin) {
            Some(location) => self.set_location_and_center(location),
            None => self.set_location_and_scroll(origin),
        }
        self.needs_redraw = true;
    }

    /// Moves the caret to the next occurrence of the open live search
    /// pattern — the first match after the current one — scanning forward
    /// and wrapping to the top of the buffer, and scrolls it into view.
    /// Does nothing while no search is open, the pattern is empty, or the
    /// pattern matches nothing in the buffer.
    pub fn search_next(&mut self) {
        let Some(pattern) = self.active_search_pattern() else {
            return;
        };
        // The match begins at the caret and spans one grapheme per
        // pattern grapheme, so the next match starts after the match's
        // end.
        let start = Location {
            x: self
                .location
                .x
                .saturating_add(Self::pattern_grapheme_count(pattern)),
            y: self.location.y,
        };
        if let Some(location) = self.buffer.find_wrapping(pattern, start) {
            self.set_location_and_scroll(location);
            // The highlight follows the caret, so a moved match needs a
            // re-render even when the scroll did not change.
            self.needs_redraw = true;
        }
    }

    /// Moves the caret to the previous occurrence of the open live search
    /// pattern — the last match ending before the current one — scanning
    /// backwards and wrapping to the end of the buffer, and scrolls it
    /// into view. Does nothing while no search is open, the pattern is
    /// empty, or the pattern matches nothing in the buffer.
    pub fn search_prev(&mut self) {
        let Some(pattern) = self.active_search_pattern() else {
            return;
        };
        if let Some(location) = self.buffer.find_wrapping_before(pattern, self.location) {
            self.set_location_and_scroll(location);
            // The highlight follows the caret, so a moved match needs a
            // re-render even when the scroll did not change.
            self.needs_redraw = true;
        }
    }

    /// Clears the live search when the search prompt closes, so the buffer
    /// stops highlighting the matches.
    pub fn clear_live_search(&mut self) {
        if self.search_pattern.take().is_some() {
            self.needs_redraw = true;
        }
    }

    /// The pattern the open search prompt is searching for, if any.
    // Only the test suite observes the live search through this so far.
    #[allow(dead_code)]
    pub fn live_search_pattern(&self) -> Option<&str> {
        self.search_pattern.as_deref()
    }

    /// The pattern the open search is actually searching for: `None`
    /// while no search is open or while the pattern is still empty — an
    /// empty pattern is no search yet, so it matches and highlights
    /// nothing.
    fn active_search_pattern(&self) -> Option<&str> {
        self.search_pattern
            .as_deref()
            .filter(|pattern| !pattern.is_empty())
    }

    /// The number of graphemes in `pattern`: how far one of its
    /// occurrences extends in grapheme indices.
    fn pattern_grapheme_count(pattern: &str) -> usize {
        pattern.grapheme_indices(true).count()
    }

    /// Whether the open live search currently matches the buffer: false
    /// while no search is open, while the pattern is still empty, or
    /// while the pattern has no match.
    pub fn live_search_has_match(&self) -> bool {
        self.live_search_highlight().is_some()
    }

    /// The grapheme spans of every occurrence of the open live search
    /// pattern on `line`, or none at all while there is no active
    /// search, so no row is ever highlighted then. The spans are the
    /// line's own, so they clip to the line's content.
    fn line_match_spans(&self, line: &Line) -> Option<Vec<Range<usize>>> {
        self.active_search_pattern()
            .map(|pattern| line.match_spans(pattern))
    }

    /// The grapheme span of the active live search match on its buffer
    /// row — the occurrence the caret sits on — while the search is
    /// active and its pattern matches at the caret: `None` when there is
    /// no active search or the pattern matches nothing in the buffer
    /// (the caret then rests at the search origin, on no match).
    ///
    /// A match counts even when an earlier occurrence of the pattern
    /// exists on the same line: the search may have started after it.
    fn live_search_highlight(&self) -> Option<(usize, Range<usize>)> {
        let pattern = self.active_search_pattern()?;
        let line = self.buffer.lines.get(self.location.y)?;
        if line.find_from(pattern, self.location.x) != Some(self.location.x) {
            return None;
        }
        let start = self.location.x;
        let span = start..start.saturating_add(Self::pattern_grapheme_count(pattern));
        Some((self.location.y, span))
    }

    /// Inserts a character, moves the caret right, scrolls it into view, and
    /// marks the buffer modified and the view dirty.
    fn handle_insert(&mut self, c: char) {
        self.buffer.insert_char(self.location, c);
        self.set_location(self.location.x.saturating_add(1), self.location.y);
        self.scroll_location_into_view();
        self.needs_redraw = true;
    }

    /// Splits the line at the caret (or appends an empty line when the caret
    /// is below the last line) and moves the caret to the beginning of the
    /// new line. Forces a re-render.
    fn handle_newline(&mut self) {
        let new_location = self.buffer.newline(self.location);
        self.set_location(new_location.x, new_location.y);
        self.desired_x = 0;
        self.scroll_location_into_view();
        self.needs_redraw = true;
    }

    /// Deletes the grapheme cluster at the caret position and marks the view
    /// dirty.
    fn handle_delete(&mut self) {
        self.buffer.delete(self.location);
        self.needs_redraw = true;
    }

    fn handle_backspace(&mut self) {
        let Location { x, y } = self.location;

        // No-op at the very start of the buffer.
        if x == 0 && y == 0 {
            return;
        }

        // Move the caret one grapheme left. At x == 0 this wraps to the end of the
        // previous line, so the two lines must be joined; otherwise the caret just
        // stepped back and we delete the grapheme now to its left.
        self.move_location(Direction::Left);

        if x == 0 {
            self.buffer.consolidate_lines(y, y.saturating_sub(1));
        } else {
            self.handle_delete();
        }

        // guarantees a re-render.
        self.needs_redraw = true;
    }

    /// Associates the buffer with `file_name` and saves it to that file,
    /// reporting the outcome on the message bar. Used when the user named a
    /// previously unnamed document through the command bar.
    pub fn save_as(&mut self, file_name: &str) {
        self.buffer.file_name = Some(file_name.to_string());
        self.handle_save();
    }

    /// Writes the buffer to its file and reports the outcome on the message
    /// bar: a brief success note naming the file, or an error that stays
    /// visible until the user has read it.
    fn handle_save(&mut self) {
        match self.buffer.save() {
            Ok(()) => {
                // Briefly confirm the save on the message bar.
                let file_name = self.buffer.file_name.as_deref().unwrap_or_default();
                self.set_message(
                    format!("File saved successfully to {file_name}"),
                    DEFAULT_MESSAGE_DURATION,
                );
            }
            Err(_) => {
                // Keep the error on the bar until the user has read it:
                // it stays until a new message replaces it.
                self.set_message(String::from("Error writing file!"), LONG_MESSAGE_DURATION);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::constants::{LONG_MESSAGE_DURATION, TAB_WIDTH_SPACES};
    use crate::common::line::Line;
    use std::time::Duration;

    fn make_view(lines_data: &[&str]) -> View {
        let buffer = Buffer {
            lines: lines_data.iter().copied().map(Line::from).collect(),
            modified: true,
            file_name: None,
        };
        let terminal_size = Terminal::size().unwrap_or(Size {
            height: 24,
            width: 80,
        });
        View {
            buffer,
            needs_redraw: false,
            size: terminal_size,
            location: Location::default(),
            scroll_offset: Position::default(),
            quit_pending: false,
            desired_x: 0,
            status_bar: StatusBar::new(terminal_size),
            message_bar: MessageBar::new(terminal_size),
            search_pattern: None,
        }
    }

    #[test]
    fn new_view_shows_the_help_message_on_the_message_bar() {
        let view = View::new(Size {
            width: 80,
            height: 24,
        });
        let message = view.get_message();
        assert_eq!(
            message.message,
            "HELP: Ctrl-F = find | Ctrl-S = save | Ctrl-Q = quit"
        );
        assert_eq!(message.duration, DEFAULT_MESSAGE_DURATION);
    }

    #[test]
    fn get_message_returns_a_stable_state_across_calls() {
        let mut view = make_view(&[]);
        view.set_message(String::from("hi"), Duration::from_secs(5));
        assert_eq!(view.get_message(), view.get_message());
    }

    #[test]
    fn set_message_stores_message_and_duration() {
        let mut view = make_view(&[]);
        view.set_message(String::from("Saved!"), Duration::from_secs(5));
        let message = view.get_message();
        assert_eq!(message.message, "Saved!");
        assert_eq!(message.duration, Duration::from_secs(5));
    }

    #[test]
    fn save_with_file_name_shows_success_message_with_file_name() {
        let path =
            std::env::temp_dir().join(format!("sted_view_save_test_{}.txt", std::process::id()));
        std::fs::write(&path, "hello\n").expect("create temp file");
        let file_path = path.to_string_lossy().to_string();

        let mut view = make_view(&["hello"]);
        view.buffer.file_name = Some(file_path.clone());

        view.handle_save();

        let message = view.get_message();
        assert_eq!(
            message.message,
            format!("File saved successfully to {file_path}")
        );
        assert_eq!(message.duration, DEFAULT_MESSAGE_DURATION);
        assert!(!view.buffer.modified);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_without_file_name_shows_error_message() {
        let mut view = make_view(&["hello"]);

        view.handle_save();

        let message = view.get_message();
        assert_eq!(message.message, "Error writing file!");
        assert_eq!(message.duration, LONG_MESSAGE_DURATION);
        assert!(view.buffer.modified);
    }

    #[test]
    fn save_as_names_the_buffer_and_reports_success() {
        let path =
            std::env::temp_dir().join(format!("sted_view_save_as_{}.txt", std::process::id()));
        let file_path = path.to_string_lossy().to_string();

        let mut view = make_view(&["hello"]);
        view.save_as(&file_path);

        let message = view.get_message();
        assert_eq!(
            message.message,
            format!("File saved successfully to {file_path}")
        );
        assert_eq!(message.duration, DEFAULT_MESSAGE_DURATION);
        assert_eq!(view.buffer.file_name.as_deref(), Some(file_path.as_str()));
        assert!(!view.buffer.modified);
        assert!(path.exists());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_as_of_unwritable_path_keeps_the_name_and_reports_error() {
        let mut view = make_view(&["hello"]);

        view.save_as("/nonexistent_dir_sted/cannot_be_created.txt");

        assert_eq!(view.get_message().message, "Error writing file!");
        // The failed name is kept, so the next save retries the same path.
        assert_eq!(
            view.buffer.file_name.as_deref(),
            Some("/nonexistent_dir_sted/cannot_be_created.txt")
        );
        assert!(view.buffer.modified);
    }

    #[test]
    fn has_file_name_reflects_the_buffer() {
        let mut view = make_view(&["a"]);
        assert!(!view.has_file_name());
        view.buffer.file_name = Some(String::from("x.txt"));
        assert!(view.has_file_name());
    }

    #[test]
    fn escape_is_ignored_by_the_view() {
        let mut view = make_view(&["a"]);

        view.handle_command(EditorCommand::Escape);

        assert_eq!(view.get_message().message, "");
    }

    fn view_with_lines(count: usize) -> View {
        let mut view = make_view(&["top"]);
        view.buffer.lines = (0..count)
            .map(|i| Line::from(format!("line {i}").as_str()))
            .collect();
        view.size = Size {
            width: 80,
            height: 22,
        };
        view
    }

    #[test]
    fn find_first_moves_the_caret_to_the_first_match_at_or_after_the_caret() {
        let mut view = make_view(&["alpha", "beta", "alpha"]);
        view.set_location(0, 1);

        assert!(view.find_first("alpha"));

        assert_eq!((view.location.x, view.location.y), (0, 2));
    }

    #[test]
    fn find_first_skips_matches_before_the_caret() {
        let mut view = make_view(&["hello world hello"]);
        view.set_location(6, 0);

        assert!(view.find_first("hello"));

        assert_eq!((view.location.x, view.location.y), (12, 0));
    }

    #[test]
    fn find_first_finds_a_match_that_starts_at_the_caret() {
        let mut view = make_view(&["abcabc"]);
        view.set_location(3, 0);

        assert!(view.find_first("abc"));

        assert_eq!((view.location.x, view.location.y), (3, 0));
    }

    #[test]
    fn find_from_the_middle_of_a_later_line_searches_forward_only() {
        let mut view = make_view(&["abc", "xyz abc"]);
        view.set_location(2, 1);

        assert!(view.find_first("abc"));

        assert_eq!((view.location.x, view.location.y), (4, 1));
    }

    #[test]
    fn find_first_from_a_position_past_the_end_of_the_buffer_wraps_to_the_top() {
        let mut view = make_view(&["abc"]);
        view.set_location(0, 1); // the "one past the end" position

        assert!(view.find_first("a"));

        assert_eq!((view.location.x, view.location.y), (0, 0));
    }

    #[test]
    fn find_first_reports_the_grapheme_column_of_the_match() {
        let mut view = make_view(&["hello world"]);

        assert!(view.find_first("world"));

        assert_eq!((view.location.x, view.location.y), (6, 0));
    }

    #[test]
    fn find_first_of_a_multibyte_pattern_counts_graphemes_not_bytes() {
        // 'é' is two bytes in UTF-8 but one grapheme, so "latte" starts at
        // grapheme 5 of "café latte".
        let mut view = make_view(&["café latte"]);

        assert!(view.find_first("latte"));

        assert_eq!((view.location.x, view.location.y), (5, 0));
    }

    #[test]
    fn find_first_without_a_match_leaves_the_caret_where_it_is() {
        let mut view = make_view(&["abc"]);
        view.set_location(1, 0);

        assert!(!view.find_first("zzz"));

        assert_eq!((view.location.x, view.location.y), (1, 0));
    }

    #[test]
    fn find_first_of_an_empty_pattern_leaves_the_caret_where_it_is() {
        let mut view = make_view(&["abc"]);
        view.set_location(2, 0);

        assert!(!view.find_first(""));

        assert_eq!((view.location.x, view.location.y), (2, 0));
    }

    #[test]
    fn find_first_on_an_empty_buffer_leaves_the_caret_where_it_is() {
        let mut view = make_view(&[]);
        view.set_location(0, 0);

        assert!(!view.find_first("a"));

        assert_eq!((view.location.x, view.location.y), (0, 0));
    }

    #[test]
    fn find_first_centers_a_match_far_below_the_viewport() {
        let mut view = view_with_lines(100);

        view.find_first("line 60");

        assert_eq!((view.location.x, view.location.y), (0, 60));
        // Row 60 lands at the middle of the 22-row viewport: 60 - 22 / 2.
        assert_eq!(view.scroll_offset.row, 49);
    }

    #[test]
    fn find_first_centers_a_match_in_the_middle_of_the_document() {
        let mut view = view_with_lines(40);

        view.find_first("line 15");

        assert_eq!((view.location.x, view.location.y), (0, 15));
        // Row 15 lands at the middle of the 22-row viewport: 15 - 22 / 2.
        assert_eq!(view.scroll_offset.row, 4);
    }

    #[test]
    fn find_first_of_a_match_near_the_top_scrolls_to_the_top_not_past_it() {
        let mut view = view_with_lines(40);

        view.find_first("line 3");

        assert_eq!((view.location.x, view.location.y), (0, 3));
        // Row 3 - 22 / 2 would be negative: the viewport starts at row 0.
        assert_eq!(view.scroll_offset.row, 0);
    }

    #[test]
    fn find_first_scrolls_a_match_below_the_viewport_into_view() {
        let mut view = view_with_lines(40);
        view.set_location(0, 30);
        view.scroll_location_into_view();
        assert_eq!(view.scroll_offset.row, 9);

        view.find_first("line 39");

        // Centering wants row 39 - 22 / 2, but the document only scrolls
        // to row 40 - 22.
        assert_eq!((view.location.x, view.location.y), (0, 39));
        assert_eq!(view.scroll_offset.row, 40 - 22);
    }

    #[test]
    fn find_first_centers_the_match_at_the_middle_of_the_viewport_width() {
        let line = format!("{}XYZ{}", "a".repeat(100), "a".repeat(97));
        let mut view = make_view(&[&line]);
        view.size = Size {
            width: 80,
            height: 22,
        };

        view.find_first("XYZ");

        assert_eq!((view.location.x, view.location.y), (100, 0));
        // The match's column sits at the middle of the 80-column
        // viewport: 100 - 80 / 2.
        assert_eq!(view.scroll_offset.col, 60);
    }

    #[test]
    fn find_first_of_a_match_near_the_start_of_a_long_line_scrolls_to_column_zero() {
        let line = format!("XYZ{}", "a".repeat(194));
        let mut view = make_view(&[&line]);
        view.size = Size {
            width: 80,
            height: 22,
        };

        view.find_first("XYZ");

        assert_eq!((view.location.x, view.location.y), (0, 0));
        assert_eq!(view.scroll_offset.col, 0);
    }

    #[test]
    fn find_first_of_a_match_near_the_end_of_a_long_line_clamps_the_scroll() {
        let line = format!("{}XYZ", "a".repeat(194));
        let mut view = make_view(&[&line]);
        view.size = Size {
            width: 80,
            height: 22,
        };

        view.find_first("XYZ");

        assert_eq!((view.location.x, view.location.y), (194, 0));
        // Centering would scroll past the end of the line: the viewport
        // stops where the line ends.
        assert_eq!(view.scroll_offset.col, 197 - 80);
    }

    #[test]
    fn find_first_wraps_to_the_top_and_scrolls_the_match_into_view() {
        let mut view = view_with_lines(40);
        view.set_location(0, 30);
        view.scroll_location_into_view();
        assert_eq!(view.scroll_offset.row, 9);

        // Nothing after row 30 matches, so the search wraps to the top.
        view.find_first("line 0");

        assert_eq!((view.location.x, view.location.y), (0, 0));
        assert_eq!(view.scroll_offset.row, 0);
    }

    #[test]
    fn set_location_and_scroll_moves_the_caret_and_updates_the_desired_column() {
        let mut view = make_view(&["abc", "def"]);
        view.set_location(2, 1);

        view.set_location_and_scroll(Location { x: 1, y: 0 });

        assert_eq!((view.location.x, view.location.y), (1, 0));
        assert_eq!(view.desired_x, 1);
    }

    #[test]
    fn update_live_search_moves_the_caret_and_highlights_the_match() {
        let mut view = make_view(&["hello world"]);
        view.set_location(0, 0);

        view.update_live_search("world", Location { x: 0, y: 0 });

        assert_eq!((view.location.x, view.location.y), (6, 0));
        assert_eq!(view.live_search_highlight(), Some((0, 6..11)));
        // The highlight only appears after a re-render.
        assert!(view.needs_redraw);
    }

    #[test]
    fn update_live_search_counts_the_pattern_in_graphemes_not_bytes() {
        let mut view = make_view(&["café latte"]);
        view.set_location(0, 0);

        view.update_live_search("latte", Location { x: 0, y: 0 });

        // "latte" starts at grapheme 5 and spans five graphemes.
        assert_eq!(view.live_search_highlight(), Some((0, 5..10)));
    }

    #[test]
    fn update_live_search_of_a_pattern_without_a_match_restores_the_origin() {
        let mut view = make_view(&["abc"]);
        view.set_location(1, 0);

        view.update_live_search("zzz", Location { x: 1, y: 0 });

        assert_eq!((view.location.x, view.location.y), (1, 0));
        assert_eq!(view.live_search_highlight(), None);
        // The (cleared) highlight still needs a re-render.
        assert!(view.needs_redraw);
    }

    #[test]
    fn update_live_search_to_an_empty_pattern_clears_the_highlight() {
        let mut view = make_view(&["abc"]);
        view.update_live_search("a", Location { x: 0, y: 0 });
        assert_eq!(view.live_search_highlight(), Some((0, 0..1)));

        view.update_live_search("", Location { x: 0, y: 0 });

        assert_eq!(view.live_search_highlight(), None);
        assert!(view.needs_redraw);
    }

    #[test]
    fn update_live_search_starts_from_the_origin_not_the_top() {
        let mut view = make_view(&["foo cat foo bar"]);
        view.set_location(8, 0);

        // The first "foo" is before the origin: the match is the second one.
        view.update_live_search("foo", Location { x: 8, y: 0 });

        assert_eq!((view.location.x, view.location.y), (8, 0));
        assert_eq!(view.live_search_highlight(), Some((0, 8..11)));
    }

    #[test]
    fn update_live_search_centers_the_match_in_the_viewport() {
        let mut view = view_with_lines(40);
        view.set_location(0, 0);

        view.update_live_search("line 25", Location { x: 0, y: 0 });

        assert_eq!((view.location.x, view.location.y), (0, 25));
        // Row 25 lands at the middle of the 22-row viewport: 25 - 22 / 2.
        assert_eq!(view.scroll_offset.row, 14);
    }

    #[test]
    fn update_live_search_wraps_to_the_top_when_nothing_matches_after_the_origin() {
        let mut view = make_view(&["foo cat foo bar"]);
        view.set_location(15, 0);

        // Nothing matches at or after the origin: the search wraps to the top.
        view.update_live_search("foo", Location { x: 15, y: 0 });

        assert_eq!((view.location.x, view.location.y), (0, 0));
        assert_eq!(view.live_search_highlight(), Some((0, 0..3)));
    }

    #[test]
    fn update_live_search_of_a_pattern_that_matches_nowhere_keeps_the_caret_at_the_origin() {
        let mut view = make_view(&["foo cat foo bar"]);
        view.set_location(15, 0);

        // The pattern matches nothing in the buffer, so the caret stays there.
        view.update_live_search("zzz", Location { x: 15, y: 0 });

        assert_eq!((view.location.x, view.location.y), (15, 0));
        assert_eq!(view.live_search_highlight(), None);
        assert!(!view.live_search_has_match());
    }

    #[test]
    fn search_next_moves_the_caret_to_the_next_match_after_the_current_one() {
        let mut view = make_view(&["hello world hello"]);
        view.update_live_search("hello", Location { x: 0, y: 0 });
        assert_eq!((view.location.x, view.location.y), (0, 0));

        view.search_next();

        assert_eq!((view.location.x, view.location.y), (12, 0));
        assert_eq!(view.live_search_highlight(), Some((0, 12..17)));
        // The highlight moved, so the view must re-render even though the
        // scroll did not change.
        assert!(view.needs_redraw);
    }

    #[test]
    fn search_next_wraps_to_the_top_when_no_match_follows_the_current_one() {
        let mut view = make_view(&["hello world hello"]);
        view.update_live_search("hello", Location { x: 12, y: 0 });
        assert_eq!((view.location.x, view.location.y), (12, 0));

        view.search_next();

        assert_eq!((view.location.x, view.location.y), (0, 0));
    }

    #[test]
    fn search_next_of_a_multibyte_pattern_steps_past_the_whole_match() {
        // 'é' is two bytes in UTF-8 but one grapheme: "café" spans four
        // graphemes, so the next match starts at grapheme 5.
        let mut view = make_view(&["café café"]);
        view.update_live_search("café", Location { x: 0, y: 0 });
        assert_eq!((view.location.x, view.location.y), (0, 0));

        view.search_next();

        assert_eq!((view.location.x, view.location.y), (5, 0));
    }

    #[test]
    fn search_prev_moves_the_caret_to_the_previous_match_before_the_current_one() {
        let mut view = make_view(&["hello world hello"]);
        view.update_live_search("hello", Location { x: 12, y: 0 });
        assert_eq!((view.location.x, view.location.y), (12, 0));

        view.search_prev();

        assert_eq!((view.location.x, view.location.y), (0, 0));
        assert_eq!(view.live_search_highlight(), Some((0, 0..5)));
        assert!(view.needs_redraw);
    }

    #[test]
    fn search_prev_wraps_to_the_end_when_no_match_precedes_the_current_one() {
        let mut view = make_view(&["hello world hello"]);
        view.update_live_search("hello", Location { x: 0, y: 0 });
        assert_eq!((view.location.x, view.location.y), (0, 0));

        view.search_prev();

        assert_eq!((view.location.x, view.location.y), (12, 0));
    }

    #[test]
    fn search_next_and_search_prev_step_between_matches_across_lines() {
        let mut view = make_view(&["abc", "xyz abc"]);
        view.update_live_search("abc", Location { x: 4, y: 1 });
        assert_eq!((view.location.x, view.location.y), (4, 1));

        view.search_prev();
        assert_eq!((view.location.x, view.location.y), (0, 0));

        view.search_next();
        assert_eq!((view.location.x, view.location.y), (4, 1));
    }

    #[test]
    fn search_next_and_search_prev_without_a_match_keep_the_caret_where_it_is() {
        let mut view = make_view(&["abc"]);
        view.update_live_search("zzz", Location { x: 1, y: 0 });
        assert_eq!((view.location.x, view.location.y), (1, 0));

        view.search_next();
        view.search_prev();

        assert_eq!((view.location.x, view.location.y), (1, 0));
    }

    #[test]
    fn search_next_and_search_prev_without_an_open_search_do_nothing() {
        let mut view = make_view(&["abc abc"]);
        view.set_location(0, 0);

        view.search_next();
        view.search_prev();

        assert_eq!((view.location.x, view.location.y), (0, 0));
    }

    #[test]
    fn live_search_highlight_is_none_when_the_caret_is_not_on_a_match() {
        let mut view = make_view(&["alpha beta"]);
        view.search_pattern = Some(String::from("alpha"));
        view.set_location(7, 0); // on "beta", not on the match

        assert_eq!(view.live_search_highlight(), None);
    }

    #[test]
    fn live_search_highlight_counts_a_match_at_the_caret_after_an_earlier_occurrence() {
        let mut view = make_view(&["abc abc"]);
        view.set_location(2, 0);

        // The first "abc" is before the origin, so the match is the second one.
        view.update_live_search("abc", Location { x: 2, y: 0 });

        assert_eq!((view.location.x, view.location.y), (4, 0));
        assert_eq!(view.live_search_highlight(), Some((0, 4..7)));
    }

    #[test]
    fn line_match_spans_of_the_open_search_cover_every_occurrence_on_the_line() {
        let mut view = make_view(&["hello world hello", "no match here"]);
        view.update_live_search("hello", Location { x: 0, y: 0 });

        assert_eq!(
            view.line_match_spans(&view.buffer.lines[0]),
            Some(vec![0..5, 12..17])
        );
        // The line without an occurrence gets no spans at all.
        assert_eq!(
            view.line_match_spans(&view.buffer.lines[1]),
            Some(Vec::<Range<usize>>::new())
        );
    }

    #[test]
    fn line_match_spans_count_graphemes_not_bytes() {
        let mut view = make_view(&["café latte"]);
        view.update_live_search("café", Location { x: 0, y: 0 });

        // 'é' is one grapheme, so "café" spans four graphemes.
        let cafe = 0..4;
        assert_eq!(
            view.line_match_spans(&view.buffer.lines[0]),
            Some(vec![cafe])
        );
    }

    #[test]
    fn line_match_spans_are_none_without_an_open_search() {
        let view = make_view(&["abc abc"]);

        assert_eq!(view.line_match_spans(&view.buffer.lines[0]), None);
    }

    #[test]
    fn line_match_spans_of_an_empty_pattern_are_none() {
        let mut view = make_view(&["abc"]);
        view.update_live_search("", Location { x: 0, y: 0 });

        // An empty pattern is no search yet: nothing is highlighted.
        assert_eq!(view.line_match_spans(&view.buffer.lines[0]), None);
    }

    #[test]
    fn line_match_spans_of_a_pattern_that_matches_nowhere_are_empty() {
        let mut view = make_view(&["abc"]);
        view.update_live_search("zzz", Location { x: 0, y: 0 });

        assert_eq!(
            view.line_match_spans(&view.buffer.lines[0]),
            Some(Vec::<Range<usize>>::new())
        );
    }

    #[test]
    fn line_match_spans_are_none_after_the_search_is_cleared() {
        let mut view = make_view(&["abc abc"]);
        view.update_live_search("abc", Location { x: 0, y: 0 });
        view.clear_live_search();

        // Dismissing the search removes the highlights again.
        assert_eq!(view.line_match_spans(&view.buffer.lines[0]), None);
    }

    #[test]
    fn the_active_match_is_one_of_the_line_matches() {
        let mut view = make_view(&["hello world hello"]);
        view.update_live_search("hello", Location { x: 0, y: 0 });

        // The active match stands out, but it is still one of the line's
        // matches: the highlight never marks more than the search does.
        let (_, active) = view.live_search_highlight().expect("the search matches");
        let spans = view
            .line_match_spans(&view.buffer.lines[0])
            .expect("the line has matches");
        assert!(spans.contains(&active));
    }

    #[test]
    fn clear_live_search_removes_the_highlight_and_marks_the_view() {
        let mut view = make_view(&["abc"]);
        view.update_live_search("a", Location { x: 0, y: 0 });
        view.needs_redraw = false;

        view.clear_live_search();

        assert_eq!(view.live_search_highlight(), None);
        assert!(view.needs_redraw);
    }

    #[test]
    fn clearing_the_live_search_and_restoring_the_caret_keeps_the_view_dirty() {
        let mut view = make_view(&["ab"]);
        view.set_location(0, 0);

        // The live search finds "b" and marks the view dirty; a render
        // pass then consumes the flag, so the highlight is on screen.
        view.update_live_search("b", Location { x: 0, y: 0 });
        assert_eq!((view.location.x, view.location.y), (1, 0));
        assert!(view.needs_redraw);
        view.needs_redraw = false;

        // Dismissing the search: clear the highlight, then restore the
        // caret where the search began — without a scroll change in
        // between.
        view.clear_live_search();
        view.set_location_and_scroll(Location { x: 0, y: 0 });

        // The view must stay dirty, or the last rendered frame still
        // shows the highlighted match.
        assert!(view.needs_redraw);
    }

    #[test]
    fn clear_live_search_without_an_open_search_is_a_noop() {
        let mut view = make_view(&["abc"]);
        view.needs_redraw = false;

        view.clear_live_search();

        assert!(!view.needs_redraw);
    }

    #[test]
    fn live_search_has_match_reflects_the_open_search() {
        let mut view = make_view(&["abc"]);
        assert!(!view.live_search_has_match()); // no search open

        view.update_live_search("b", Location { x: 0, y: 0 });
        assert!(view.live_search_has_match());

        view.update_live_search("zzz", Location { x: 0, y: 0 });
        assert!(!view.live_search_has_match());

        view.update_live_search("", Location { x: 0, y: 0 });
        assert!(!view.live_search_has_match());
    }

    #[test]
    fn load_of_missing_file_reports_error_on_message_bar() {
        let path =
            std::env::temp_dir().join(format!("sted_view_load_missing_{}.txt", std::process::id()));
        let file_path = path.to_string_lossy().to_string();

        let mut view = make_view(&["existing content"]);

        view.load(&file_path);

        let message = view.get_message();
        assert_eq!(
            message.message,
            format!("Error: Could not open file: {file_path}")
        );
        assert_eq!(message.duration, LONG_MESSAGE_DURATION);
        // The buffer falls back to empty, unsaved state.
        assert!(view.buffer.is_empty());
        assert!(view.buffer.file_name.is_none());
    }

    #[test]
    fn load_of_existing_file_replaces_buffer_and_keeps_name() {
        let path = std::env::temp_dir().join(format!("sted_view_load_{}.txt", std::process::id()));
        std::fs::write(&path, "one\ntwo\n").expect("create temp file");
        let file_path = path.to_string_lossy().to_string();

        let mut view = make_view(&["existing content"]);

        view.load(&file_path);

        let texts: Vec<String> = view.buffer.lines.iter().map(ToString::to_string).collect();
        assert_eq!(texts, vec!["one", "two"]);
        assert_eq!(view.buffer.file_name.as_deref(), Some(file_path.as_str()));
        // A successful load must not touch the message bar.
        assert_eq!(view.get_message().message, "");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn quit_without_unsaved_changes_exits_immediately() {
        let mut view = make_view(&["abc"]);
        view.buffer.modified = false;

        assert!(view.handle_quit());
        // No warning should have been raised.
        assert_eq!(view.get_message().message, "");
    }

    #[test]
    fn quit_with_unsaved_changes_warns_and_refuses_first_request() {
        let mut view = make_view(&["abc"]);
        view.buffer.file_name = Some(String::from("file.txt"));

        assert!(!view.handle_quit());

        let message = view.get_message();
        assert_eq!(
            message.message,
            "WARNING! File file.txt has unsaved changes. Press Ctrl-Q again to quit without saving"
        );
        assert_eq!(message.duration, LONG_MESSAGE_DURATION);
    }

    #[test]
    fn quit_with_unsaved_changes_and_no_file_name_names_it_unnamed() {
        let mut view = make_view(&["abc"]);

        assert!(!view.handle_quit());
        assert_eq!(
            view.get_message().message,
            "WARNING! File (unnamed) has unsaved changes. Press Ctrl-Q again to quit without saving"
        );
    }

    #[test]
    fn second_consecutive_quit_request_exits() {
        let mut view = make_view(&["abc"]);

        assert!(!view.handle_quit());
        assert!(view.handle_quit());
    }

    #[test]
    fn other_command_resets_pending_quit_warning() {
        let mut view = make_view(&["abc"]);

        assert!(!view.handle_quit()); // first press: warn
        view.handle_command(EditorCommand::Insert('x')); // activity in between
        assert!(!view.handle_quit()); // must warn again
        assert!(view.handle_quit()); // and the next press confirms
    }

    #[test]
    fn saving_clears_unsaved_changes_and_allows_immediate_quit() {
        let path = std::env::temp_dir().join(format!("sted_view_quit_{}.txt", std::process::id()));
        std::fs::write(&path, "abc\n").expect("create temp file");
        let file_path = path.to_string_lossy().to_string();

        let mut view = make_view(&["abc"]);
        view.buffer.file_name = Some(file_path.clone());

        assert!(!view.handle_quit()); // first press: warn
        view.handle_save(); // user saves instead
        assert!(view.handle_quit()); // nothing left to warn about

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn resize_shrink_updates_viewport_and_marks_the_view_dirty() {
        let mut view = make_view(&["a", "b"]);
        view.needs_redraw = false;

        view.resize(Size {
            width: 80,
            height: 20,
        });

        assert_eq!(
            view.size.height,
            20usize.saturating_sub(RESERVED_BOTTOM_LINES)
        );
        assert_eq!(view.size.width, 80);
        assert!(view.needs_redraw);
    }

    #[test]
    fn handle_delete_marks_the_view_dirty() {
        let mut view = make_view(&["abc"]);
        assert!(!view.needs_redraw); // a render clears the dirty flag

        view.handle_delete();
        assert!(view.needs_redraw);
    }

    #[test]
    fn handle_delete_removes_grapheme_from_current_line() {
        let mut view = make_view(&["hello"]);
        view.set_location(0, 0);

        view.handle_delete(); // deletes grapheme at index 0 ('h')
        assert_eq!(view.buffer.lines[0].grapheme_count(), 4);
        let text: String = view.buffer.lines[0].to_string();
        assert!(!text.contains('h'));
    }

    #[test]
    fn handle_delete_on_nonzero_location() {
        let mut view = make_view(&["abcdef"]);
        view.set_location(3, 0); // caret between 'd' and 'e'

        view.handle_delete(); // deletes grapheme at index 3 ('d')
        let text: String = view.buffer.lines[0].to_string();
        assert!(!text.contains('d'));
    }

    #[test]
    fn handle_delete_with_out_of_bounds_location() {
        let mut view = make_view(&["abc"]);
        // Location is (0, 0) which is within bounds — buffer.delete handles row-OOB internally
        view.handle_delete();
        assert!(view.needs_redraw);
        let text: String = view.buffer.lines[0].to_string();
        assert!(!text.contains('a'));
    }

    #[test]
    fn handle_delete_across_multiple_lines() {
        let mut view = make_view(&["first", "second"]);
        let first_len_before = view.buffer.lines[0].grapheme_count();
        view.set_location(1, 0); // delete from first line
        view.handle_delete();
        assert_eq!(view.buffer.lines[0].grapheme_count(), first_len_before - 1);

        let second_len_before = view.buffer.lines[1].grapheme_count();
        view.set_location(2, 1); // delete from second line
        view.handle_delete();
        assert_eq!(view.buffer.lines[1].grapheme_count(), second_len_before - 1);

        assert!(view.needs_redraw);
    }

    #[test]
    fn backspace_at_beginning_of_line_merges_with_previous() {
        let mut view = make_view(&["ab", "cd"]);
        view.set_location(0, 1);

        view.handle_backspace();

        assert_eq!(view.buffer.lines.len(), 1);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "abcd");
        // caret lands at the end of the previous line
        assert_eq!((view.location.x, view.location.y), (2, 0));
        assert!(view.needs_redraw);
    }

    #[test]
    fn backspace_at_first_line_starts_nothing() {
        let mut view = make_view(&["ab"]);
        view.set_location(0, 0);

        view.handle_backspace();

        assert_eq!(view.buffer.lines.len(), 1);
        assert_eq!(view.buffer.lines[0].to_string(), "ab");
    }

    #[test]
    fn backspace_with_empty_previous_line_keeps_content() {
        let mut view = make_view(&["", "cd"]);
        view.set_location(0, 1);

        view.handle_backspace();

        assert_eq!(view.buffer.lines.len(), 1);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "cd");
        // caret lands at the end of the (empty) previous line
        assert_eq!((view.location.x, view.location.y), (0, 0));
    }

    #[test]
    fn backspace_mid_line_deletes_left_grapheme() {
        let mut view = make_view(&["abcd"]);
        view.set_location(2, 0);

        view.handle_backspace();

        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "acd");
        assert_eq!((view.location.x, view.location.y), (1, 0));
    }

    #[test]
    fn handle_insert_marks_the_view_dirty() {
        let mut view = make_view(&["hello"]);
        assert!(!view.needs_redraw); // a render clears the dirty flag

        view.handle_insert('x');
        assert!(view.needs_redraw);
    }

    #[test]
    fn handle_insert_moves_caret_right_by_one_column() {
        let mut view = make_view(&["hello"]);
        view.set_location(2, 0);
        assert_eq!(view.location.x, 2);

        view.handle_insert('x');
        assert_eq!(view.location.x, 3);
        assert_eq!(view.location.y, 0); // row should not change
    }

    #[test]
    fn handle_insert_at_beginning_of_line() {
        let mut view = make_view(&["hello"]);
        view.set_location(0, 0);

        view.handle_insert('x');
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "xhello");
    }

    #[test]
    fn handle_insert_at_end_of_line() {
        let mut view = make_view(&["hello"]);
        let line_len = view.buffer.lines[0].grapheme_count();
        view.set_location(line_len, 0); // position at end of "hello"

        view.handle_insert('!');
        assert_eq!(view.location.x, line_len + 1);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "hello!");
    }

    #[test]
    fn handle_insert_in_middle_of_line() {
        let mut view = make_view(&["hello world"]);
        view.set_location(5, 0); // position at the space

        view.handle_insert('|');
        assert_eq!(view.location.x, 6);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "hello| world");
    }

    #[test]
    fn handle_insert_on_empty_line() {
        let mut view = make_view(&[""]);
        view.set_location(0, 0);

        view.handle_insert('a');
        assert_eq!(view.location.x, 1);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "a");
    }

    #[test]
    fn handle_insert_on_empty_lines() {
        let mut view = make_view(&["", ""]);
        view.set_location(0, 0);

        view.handle_insert('x');
        assert_eq!(view.location.x, 1);
        view.handle_insert('y');
        assert_eq!(view.location.x, 2);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "xy");
    }

    #[test]
    fn handle_insert_on_different_rows() {
        let mut view = make_view(&["abc", "def"]);

        // Insert on first line
        view.set_location(0, 0);
        view.handle_insert('A');
        assert_eq!(view.location.x, 1);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "Aabc");

        // Move to second line and insert
        view.set_location(1, 1); // after 'd'
        view.handle_insert('X');
        assert_eq!(view.location.x, 2);
        let text: String = view.buffer.lines[1].to_string();
        assert_eq!(text, "dXef");
    }

    #[test]
    fn handle_insert_does_not_affect_other_lines() {
        let mut view = make_view(&["first", "middle", "last"]);
        let first_len_before = view.buffer.lines[0].grapheme_count();
        let last_len_before = view.buffer.lines[2].grapheme_count();
        view.set_location(2, 1); // middle line ("middle" has 6 graphemes)

        view.handle_insert('x');

        // Only the middle line should change — its length goes from 6 to 7
        assert_eq!(view.buffer.lines[1].grapheme_count(), 7);
        assert_eq!(view.buffer.lines[0].grapheme_count(), first_len_before);
        assert_eq!(view.buffer.lines[2].grapheme_count(), last_len_before);
    }

    #[test]
    fn handle_insert_with_multicharacter_graphemes() {
        let mut view = make_view(&["café"]);
        // 'f' is at grapheme index 3, insert '!' after the é (index 4)
        view.set_location(4, 0);

        view.handle_insert('!');
        assert_eq!(view.location.x, 5);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "café!");
    }

    #[test]
    fn handle_insert_tab_inserts_a_tab_grapheme() {
        let mut view = make_view(&["ab"]);
        view.set_location(1, 0);

        view.handle_insert('\t');

        let line = &view.buffer.lines[0];
        assert_eq!(line.grapheme_count(), 3);
        let text: String = line.to_string();
        assert_eq!(text, "a\tb");
        assert_eq!((view.location.x, view.location.y), (2, 0));
    }

    #[test]
    fn handle_insert_tab_on_blank_line_moves_caret_by_tab_width_columns() {
        let mut view = make_view(&[""]);
        view.set_location(0, 0);

        view.handle_insert('\t');

        // The caret is one grapheme in, but that grapheme (a tab) renders as
        // TAB_WIDTH_SPACES columns, so the on-screen caret column must match.
        assert_eq!(view.get_caret_position().col, TAB_WIDTH_SPACES);
        assert_eq!(view.location.y, 0);
    }

    #[test]
    fn handle_newline_splits_line_at_caret_and_moves_caret_to_new_line() {
        let mut view = make_view(&["abcdef"]);
        view.set_location(3, 0);

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 2);
        let top: String = view.buffer.lines[0].to_string();
        let bottom: String = view.buffer.lines[1].to_string();
        assert_eq!(top, "abc");
        assert_eq!(bottom, "def");
        assert_eq!((view.location.x, view.location.y), (0, 1));
        assert!(view.needs_redraw);
    }

    #[test]
    fn handle_newline_at_beginning_of_line_puts_empty_line_above() {
        let mut view = make_view(&["abcdef"]);
        view.set_location(0, 0);

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 2);
        assert_eq!(view.buffer.lines[0].grapheme_count(), 0);
        let bottom: String = view.buffer.lines[1].to_string();
        assert_eq!(bottom, "abcdef");
        assert_eq!((view.location.x, view.location.y), (0, 1));
    }

    #[test]
    fn handle_newline_at_end_of_line_puts_empty_line_below() {
        let mut view = make_view(&["abcdef"]);
        view.set_location(6, 0);

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 2);
        let top: String = view.buffer.lines[0].to_string();
        assert_eq!(top, "abcdef");
        assert_eq!(view.buffer.lines[1].grapheme_count(), 0);
        assert_eq!((view.location.x, view.location.y), (0, 1));
    }

    #[test]
    fn handle_newline_at_bottom_of_document_appends_a_single_empty_line() {
        let mut view = make_view(&["abc", "def"]);
        view.set_location(0, 2); // the "one past the end" position

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 3);
        let texts: Vec<String> = view.buffer.lines.iter().map(ToString::to_string).collect();
        assert_eq!(texts, vec!["abc", "def", ""]);
        assert_eq!((view.location.x, view.location.y), (0, 2));
    }

    #[test]
    fn handle_newline_on_empty_buffer_appends_a_single_empty_line() {
        let mut view = make_view(&[]);
        view.set_location(0, 0);

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 1);
        assert_eq!(view.buffer.lines[0].grapheme_count(), 0);
        assert_eq!((view.location.x, view.location.y), (0, 0));
    }

    #[test]
    fn handle_newline_in_middle_of_document_does_not_touch_other_lines() {
        let mut view = make_view(&["first", "second", "third"]);
        view.set_location(2, 1);

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 4);
        let texts: Vec<String> = view.buffer.lines.iter().map(ToString::to_string).collect();
        assert_eq!(texts, vec!["first", "se", "cond", "third"]);
        assert_eq!((view.location.x, view.location.y), (0, 2));
        assert!(view.needs_redraw);
    }

    #[test]
    fn handle_newline_keeps_tab_in_upper_line_when_splitting_after_it() {
        let mut view = make_view(&["ab\txyz"]);
        view.set_location(3, 0); // between the tab and 'x'

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 2);
        let top: String = view.buffer.lines[0].to_string();
        let bottom: String = view.buffer.lines[1].to_string();
        assert_eq!(top, "ab\t");
        assert_eq!(bottom, "xyz");
    }
}
