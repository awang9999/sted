use crate::common::{line::Line, types::Location};
use std::fs::{File, read_to_string};
use std::io::Write;

#[derive(Clone, Default)]
pub struct Buffer {
    pub lines: Vec<Line>,
    /// True when the buffer content differs from what is on disk (unsaved changes).
    pub modified: bool,
    /// The path of the file this buffer was loaded from, if any.
    /// `None` means the buffer is a new, unsaved document.
    pub file_name: Option<String>,
}

impl Buffer {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The location of the first occurrence of `pattern` at or after
    /// `start`, scanning the lines from there onward: the line of `start`
    /// from `start.x`, then each following line from its beginning.
    /// `None` when no line contains the pattern there. An empty pattern
    /// is not a search and matches nothing.
    pub fn find_from(&self, pattern: &str, start: Location) -> Option<Location> {
        if pattern.is_empty() {
            return None;
        }
        let start_y = start.y.min(self.lines.len());
        if let Some(x) = self.lines.get(start_y)?.find_from(pattern, start.x) {
            return Some(Location { x, y: start_y });
        }
        self.lines
            .iter()
            .enumerate()
            .skip(start_y.saturating_add(1))
            .find_map(|(y, line)| line.find(pattern).map(|x| Location { x, y }))
    }

    /// The location of the first occurrence of `pattern` at or after
    /// `start`, scanning forward from there; when nothing matches after
    /// `start`, the search wraps to the top of the buffer and scans
    /// forward from there, so the match — if any — sits before `start`.
    /// `None` only when no line contains the pattern at all. An empty
    /// pattern is not a search and matches nothing.
    pub fn find_wrapping(&self, pattern: &str, start: Location) -> Option<Location> {
        if let Some(location) = self.find_from(pattern, start) {
            return Some(location);
        }
        self.find_from(pattern, Location::default())
    }

    /// The location of the last occurrence of `pattern` ending at or
    /// before `before`, scanning the lines backwards from there: the
    /// line of `before` up to `before.x`, then each preceding line, its
    /// last occurrence. `None` when no line contains the pattern before
    /// `before`. An empty pattern is not a search and matches nothing.
    pub fn find_before(&self, pattern: &str, before: Location) -> Option<Location> {
        if pattern.is_empty() {
            return None;
        }
        let before_y = before.y.min(self.lines.len());
        if before_y < self.lines.len()
            && let Some(x) = self.lines[before_y].rfind_before(pattern, before.x)
        {
            return Some(Location { x, y: before_y });
        }
        self.rfind_up_to(pattern, before_y)
    }

    /// The location of the last occurrence of `pattern` ending at or
    /// before `before`, scanning the lines backwards from there; when
    /// nothing matches before `before`, the search wraps to the end of
    /// the buffer and scans backwards from there, so the match — if any —
    /// sits after `before`. `None` only when no line contains the pattern
    /// at all. An empty pattern is not a search and matches nothing.
    pub fn find_wrapping_before(&self, pattern: &str, before: Location) -> Option<Location> {
        if pattern.is_empty() {
            return None;
        }
        self.find_before(pattern, before)
            .or_else(|| self.rfind_up_to(pattern, self.lines.len()))
    }

    /// The location of the last occurrence of `pattern` on the lines
    /// `0..upper`, scanning them backwards, or `None` when none of those
    /// lines contains the pattern.
    fn rfind_up_to(&self, pattern: &str, upper: usize) -> Option<Location> {
        (0..upper.min(self.lines.len()))
            .rev()
            .find_map(|y| self.lines[y].rfind(pattern).map(|x| Location { x, y }))
    }

    /// Reads the file at `file_path` into a new buffer.
    ///
    /// Returns an error with a user-facing message if the file could not be
    /// opened; the caller reports that message on the message bar and keeps
    /// an empty buffer.
    pub fn load(file_path: &str) -> Result<Self, String> {
        let file_content = read_to_string(file_path)
            .map_err(|_| format!("Error: Could not open file: {file_path}"))?;
        let lines: Vec<Line> = file_content.lines().map(Line::from).collect();

        Ok(Self {
            lines,
            modified: false,
            file_name: Some(file_path.to_string()),
        })
    }

    /// Writes all lines of the buffer to the file it was loaded from.
    /// Returns an error if the buffer has no file name (e.g. it is a new document).
    pub fn save(&mut self) -> Result<(), std::io::Error> {
        if let Some(file_name) = &self.file_name {
            let mut file = File::create(file_name)?;
            for line in &self.lines {
                // The line's Display writes its text verbatim, so saving
                // writes it directly without re-assembling anything.
                writeln!(file, "{line}")?;
            }
            self.modified = false;
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No file name is associated with this buffer.",
            ))
        }
    }

    pub fn get_col_from_text_location(&self, location: Location) -> usize {
        self.lines
            .get(location.y)
            .map_or(0, |line| line.width_until(location.x))
    }

    /// Inserts character `c` at column `location.x` on row `location.y`. Creates a new line if the row exceeds the buffer length.
    pub fn insert_char(&mut self, location: Location, c: char) {
        if location.y >= self.lines.len() {
            self.lines.push(Line::from_char(c));
        } else {
            self.lines[location.y].insert_char(location.x, c);
        }
        self.modified = true;
    }

    /// Handles Enter pressed at `location` and returns the location the caret
    /// should move to (always column 0 of the new line):
    /// - within a line, the line is split at the caret and everything after
    ///   the caret becomes the new line below;
    /// - at the beginning of a line, a new empty line is placed before the
    ///   current one (the current content moves down);
    /// - at the end of a line, a new empty line is placed after it;
    /// - below the last line, a new empty line is appended to the document.
    pub fn newline(&mut self, location: Location) -> Location {
        // Enter below the last line (a position we allow navigating to):
        // append a single empty line to the end of the document and place
        // the caret on it.
        if location.y >= self.lines.len() {
            let new_line_index = self.lines.len();
            self.lines.push(Line::newline());
            self.modified = true;
            return Location {
                x: 0,
                y: new_line_index,
            };
        }

        // Clamp to the line's length so Enter at or past the end of a line
        // simply appends an empty line below it.
        let index = location.x.min(self.lines[location.y].grapheme_count());
        let after_caret = self.lines[location.y].split_at(index);
        let new_line_index = location.y.saturating_add(1);
        self.lines.insert(new_line_index, after_caret);
        self.modified = true;

        Location {
            x: 0,
            y: new_line_index,
        }
    }

    /// Deletes the grapheme cluster at column `location.x` on row `location.y`.
    pub fn delete(&mut self, location: Location) {
        if let Some(line) = self.lines.get_mut(location.y) {
            line.delete_grapheme(location.x);
            self.modified = true;
        }
    }

    /// Merges the line at `source_index` into the line at `target_index`
    /// (appending the source content to the end of the target), then removes the source line.
    pub fn consolidate_lines(&mut self, source_index: usize, target_index: usize) {
        if source_index == target_index
            || source_index >= self.lines.len()
            || target_index >= self.lines.len()
        {
            return;
        }
        let source = self.lines.remove(source_index);
        self.lines[target_index].append(&source);
        self.modified = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::Location;

    fn make_buffer(lines_data: &[&str]) -> Buffer {
        Buffer {
            lines: lines_data.iter().copied().map(Line::from).collect(),
            modified: false,
            file_name: None,
        }
    }

    #[test]
    fn delete_removes_grapheme_on_existing_line() {
        let mut buf = make_buffer(&["abcd"]);
        // Location (0, 0) → deletes grapheme at index 0 from line[0] ('a')
        buf.delete(Location { x: 0, y: 0 });
        assert_eq!(buf.lines[0].to_string(), "bcd");
    }

    #[test]
    fn delete_removes_correct_position_at_nonzero_x() {
        let mut buf = make_buffer(&["abcdef"]);
        // Location (3, 0) → deletes grapheme at index 3 from line[0] ('d')
        buf.delete(Location { x: 3, y: 0 });
        assert_eq!(buf.lines[0].to_string(), "abcef");
    }

    #[test]
    fn delete_on_last_grapheme() {
        let mut buf = make_buffer(&["abc"]);
        // Location (2, 0) → deletes grapheme at index 2 ('c')
        buf.delete(Location { x: 2, y: 0 });
        assert_eq!(buf.lines[0].to_string(), "ab");
    }

    #[test]
    fn delete_handles_out_of_bounds_row() {
        let mut buf = make_buffer(&["abc", "def"]);
        // Row 5 doesn't exist → no-op
        buf.delete(Location { x: 0, y: 5 });
        assert_eq!(buf.lines.len(), 2);
    }

    #[test]
    fn delete_with_empty_lines() {
        let mut buf = make_buffer(&["abc", "", "def"]);
        // Delete a grapheme from the empty second line — there's nothing to remove
        buf.delete(Location { x: 0, y: 1 });
        assert_eq!(buf.lines[1].grapheme_count(), 0);
    }

    #[test]
    fn delete_grapheme_index_beyond_line_length_is_noop() {
        let mut buf = make_buffer(&["ab"]);
        // Location (5, 0) → grapheme_index = 6 > line length of 2 → no-op
        buf.delete(Location { x: 5, y: 0 });
        assert_eq!(buf.lines[0].grapheme_count(), 2);
    }

    #[test]
    fn delete_across_multiple_lines() {
        let mut buf = make_buffer(&["hello", "world"]);
        buf.delete(Location { x: 0, y: 0 }); // deletes from first line
        assert_eq!(buf.lines[0].to_string(), "ello");

        let second_before = buf.lines[1].grapheme_count();
        buf.delete(Location { x: 2, y: 1 }); // deletes from second line
        assert_eq!(buf.lines[1].grapheme_count(), second_before - 1);
    }

    #[test]
    fn consolidate_lines_appends_source_after_target() {
        let mut buf = make_buffer(&["ab", "cd"]);
        // Merge line 1 ("cd") into line 0 ("ab")
        buf.consolidate_lines(1, 0);
        assert_eq!(buf.lines.len(), 1);
        assert_eq!(buf.lines[0].to_string(), "abcd");
    }

    #[test]
    fn consolidate_lines_with_empty_target_keeps_source_content() {
        let mut buf = make_buffer(&["", "cd"]);
        buf.consolidate_lines(1, 0);
        assert_eq!(buf.lines.len(), 1);
        assert_eq!(buf.lines[0].to_string(), "cd");
    }

    #[test]
    fn consolidate_lines_same_index_is_noop() {
        let mut buf = make_buffer(&["abc", "def"]);
        buf.consolidate_lines(1, 1);
        assert_eq!(buf.lines.len(), 2);
    }

    #[test]
    fn consolidate_lines_out_of_bounds_is_noop() {
        let mut buf = make_buffer(&["abc"]);
        buf.consolidate_lines(0, 5);
        buf.consolidate_lines(5, 0);
        assert_eq!(buf.lines.len(), 1);
    }

    #[test]
    fn insert_char_on_empty_buffer_creates_new_line() {
        let mut buf = Buffer::default();
        assert!(buf.lines.is_empty());

        buf.insert_char(Location { x: 0, y: 0 }, 'x');
        assert_eq!(buf.lines.len(), 1);
        let text: String = buf.lines[0].to_string();
        assert_eq!(text, "x");
    }

    #[test]
    fn insert_char_into_existing_row() {
        let mut buf = make_buffer(&["abc"]);

        buf.insert_char(Location { x: 2, y: 0 }, 'X');
        assert_eq!(buf.lines[0].grapheme_count(), 4);
        let text: String = buf.lines[0].to_string();
        assert_eq!(text, "abXc");
    }

    #[test]
    fn insert_char_at_beginning_of_line() {
        let mut buf = make_buffer(&["hello"]);

        buf.insert_char(Location { x: 0, y: 0 }, 'W');
        assert_eq!(buf.lines[0].grapheme_count(), 6);
        let text: String = buf.lines[0].to_string();
        assert_eq!(text, "Whello");
    }

    #[test]
    fn insert_char_at_end_of_existing_line() {
        let mut buf = make_buffer(&["hello"]);
        let len_before = buf.lines[0].grapheme_count();

        // insert at the end (x == length of line)
        buf.insert_char(
            Location {
                x: len_before,
                y: 0,
            },
            '!',
        );
        assert_eq!(buf.lines[0].grapheme_count(), len_before + 1);
        let text: String = buf.lines[0].to_string();
        assert_eq!(text, "hello!");
    }

    #[test]
    fn insert_char_appends_when_row_does_not_exist() {
        let mut buf = make_buffer(&["abc", "def"]);

        // row 2 does not exist — should create a new line as the third element
        buf.insert_char(Location { x: 0, y: 3 }, 'x');
        assert_eq!(buf.lines.len(), 3);
    }

    #[test]
    fn insert_char_in_middle_of_existing_line_preserves_others() {
        let mut buf = make_buffer(&["abc", "middle", "last"]);
        let first_before = buf.lines[0].grapheme_count();
        let third_before = buf.lines[2].grapheme_count();

        buf.insert_char(Location { x: 1, y: 1 }, 'X');

        // only the target line changes length
        assert_eq!(buf.lines[1].grapheme_count(), 6 + 1);
        assert_eq!(buf.lines[0].grapheme_count(), first_before);
        assert_eq!(buf.lines[2].grapheme_count(), third_before);
    }

    #[test]
    fn insert_char_does_nothing_out_of_bounds_row_when_buffer_empty() {
        let mut buf = Buffer::default();

        // y=5, buffer is empty (len=0), so location.y >= lines.len → creates a new line!
        buf.insert_char(Location { x: 0, y: 5 }, 'x');
        assert_eq!(buf.lines.len(), 1);
        let text: String = buf.lines[0].to_string();
        assert_eq!(text, "x");
    }

    #[test]
    fn insert_char_with_special_characters() {
        let mut buf = make_buffer(&["abc"]);

        // Insert an emoji grapheme
        buf.insert_char(Location { x: 2, y: 0 }, '👍');
        assert_eq!(buf.lines[0].grapheme_count(), 4);
        let text: String = buf.lines[0].to_string();
        assert!(text.contains("👍"));
    }

    #[test]
    fn insert_char_into_empty_line() {
        let mut buf = make_buffer(&["", "abc"]);

        // Insert into the empty first line; the other line must be untouched
        buf.insert_char(Location { x: 0, y: 0 }, 'Z');
        assert_eq!(buf.lines.len(), 2);
        let first: String = buf.lines[0].to_string();
        assert_eq!(first, "Z");
        let second: String = buf.lines[1].to_string();
        assert_eq!(second, "abc");
    }

    // Note: insert_char does NOT silently ignore out-of-bounds. Instead:
    // - When `location.y >= lines.len()` it *creates* a new line at that index.
    // - The Line::insert_char inside gets x past the end of the line, which is okay (inserts at EOF).

    #[test]
    fn newline_splits_line_in_middle() {
        let mut buf = make_buffer(&["abcdef"]);
        let caret = buf.newline(Location { x: 3, y: 0 });
        assert_eq!(buf.lines.len(), 2);
        assert_eq!(buf.lines[0].to_string(), "abc");
        assert_eq!(buf.lines[1].to_string(), "def");
        assert_eq!((caret.x, caret.y), (0, 1));
    }

    #[test]
    fn newline_at_beginning_of_line_places_empty_line_before() {
        let mut buf = make_buffer(&["abcdef"]);
        let caret = buf.newline(Location { x: 0, y: 0 });
        assert_eq!(buf.lines.len(), 2);
        assert_eq!(buf.lines[0].grapheme_count(), 0);
        assert_eq!(buf.lines[1].to_string(), "abcdef");
        assert_eq!((caret.x, caret.y), (0, 1));
    }

    #[test]
    fn newline_at_end_of_line_places_empty_line_after() {
        let mut buf = make_buffer(&["abcdef"]);
        let caret = buf.newline(Location { x: 6, y: 0 });
        assert_eq!(buf.lines.len(), 2);
        assert_eq!(buf.lines[0].to_string(), "abcdef");
        assert_eq!(buf.lines[1].grapheme_count(), 0);
        assert_eq!((caret.x, caret.y), (0, 1));
    }

    #[test]
    fn newline_with_caret_past_end_of_line_is_treated_as_end_of_line() {
        let mut buf = make_buffer(&["ab"]);
        let caret = buf.newline(Location { x: 5, y: 0 });
        assert_eq!(buf.lines.len(), 2);
        assert_eq!(buf.lines[0].to_string(), "ab");
        assert_eq!(buf.lines[1].grapheme_count(), 0);
        assert_eq!((caret.x, caret.y), (0, 1));
    }

    #[test]
    fn newline_below_last_line_appends_a_single_empty_line() {
        let mut buf = make_buffer(&["abc", "def"]);
        // Row 2 is the "one past the end" position we allow navigating to
        let caret = buf.newline(Location { x: 0, y: 2 });
        assert_eq!(buf.lines.len(), 3);
        assert_eq!(buf.lines[0].to_string(), "abc");
        assert_eq!(buf.lines[1].to_string(), "def");
        assert_eq!(buf.lines[2].grapheme_count(), 0);
        assert_eq!((caret.x, caret.y), (0, 2));
    }

    #[test]
    fn newline_on_empty_buffer_appends_a_single_empty_line() {
        let mut buf = Buffer::default();
        let caret = buf.newline(Location { x: 0, y: 0 });
        assert_eq!(buf.lines.len(), 1);
        assert_eq!(buf.lines[0].grapheme_count(), 0);
        assert_eq!((caret.x, caret.y), (0, 0));
    }

    #[test]
    fn newline_in_middle_of_document_leaves_other_lines_untouched() {
        let mut buf = make_buffer(&["first", "second", "third"]);
        let caret = buf.newline(Location { x: 2, y: 1 });
        assert_eq!(buf.lines.len(), 4);
        assert_eq!(buf.lines[0].to_string(), "first");
        assert_eq!(buf.lines[1].to_string(), "se");
        assert_eq!(buf.lines[2].to_string(), "cond");
        assert_eq!(buf.lines[3].to_string(), "third");
        assert_eq!((caret.x, caret.y), (0, 2));
    }

    #[test]
    fn load_tracks_the_file_name() {
        let path = std::env::temp_dir().join(format!("sted_test_load_{}.txt", std::process::id()));
        std::fs::write(&path, "hello\n").unwrap();
        let file_path = path.to_string_lossy().to_string();

        let buf = Buffer::load(&file_path).expect("load should succeed");
        assert_eq!(buf.file_name.as_deref(), Some(file_path.as_str()));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_of_missing_file_is_an_error_naming_the_file() {
        let path =
            std::env::temp_dir().join(format!("sted_test_missing_{}.txt", std::process::id()));
        let file_path = path.to_string_lossy().to_string();

        let result = Buffer::load(&file_path);

        assert_eq!(
            result.err(),
            Some(format!("Error: Could not open file: {file_path}"))
        );
    }

    #[test]
    fn new_buffer_has_no_file_name() {
        let buf = Buffer::default();
        assert!(buf.file_name.is_none());
    }

    #[test]
    fn find_from_returns_the_first_match_at_or_after_the_start() {
        let buf = make_buffer(&["alpha", "beta", "alpha"]);

        assert_eq!(
            buf.find_from("alpha", Location { x: 0, y: 0 }),
            Some(Location { x: 0, y: 0 })
        );
        assert_eq!(
            buf.find_from("beta", Location { x: 0, y: 1 }),
            Some(Location { x: 0, y: 1 })
        );
    }

    #[test]
    fn find_from_skips_matches_before_the_start_position() {
        let buf = make_buffer(&["hello world hello"]);

        assert_eq!(
            buf.find_from("hello", Location { x: 6, y: 0 }),
            Some(Location { x: 12, y: 0 })
        );
        // The only "world" starts at column 6, before the start position.
        assert_eq!(buf.find_from("world", Location { x: 7, y: 0 }), None);
    }

    #[test]
    fn find_from_finds_a_match_that_starts_exactly_at_the_start_position() {
        let buf = make_buffer(&["abcabc"]);

        assert_eq!(
            buf.find_from("abc", Location { x: 3, y: 0 }),
            Some(Location { x: 3, y: 0 })
        );
    }

    #[test]
    fn find_from_on_a_later_line_skips_the_lines_before_it() {
        let buf = make_buffer(&["abc", "xyz", "abc"]);

        assert_eq!(
            buf.find_from("abc", Location { x: 1, y: 1 }),
            Some(Location { x: 0, y: 2 })
        );
    }

    #[test]
    fn find_from_at_a_position_past_the_end_of_the_buffer_is_none() {
        let buf = make_buffer(&["abc"]);

        // Row 1 is the "one past the end" position.
        assert_eq!(buf.find_from("a", Location { x: 0, y: 1 }), None);
    }

    #[test]
    fn find_from_reports_the_grapheme_column_of_the_match() {
        let buf = make_buffer(&["hello world"]);

        assert_eq!(
            buf.find_from("world", Location { x: 0, y: 0 }),
            Some(Location { x: 6, y: 0 })
        );
    }

    #[test]
    fn find_from_of_a_multibyte_pattern_counts_graphemes_not_bytes() {
        // 'é' is two bytes in UTF-8 but one grapheme, so "latte" starts at
        // grapheme 5 of "café latte".
        let buf = make_buffer(&["café latte"]);

        assert_eq!(
            buf.find_from("latte", Location { x: 0, y: 0 }),
            Some(Location { x: 5, y: 0 })
        );
    }

    #[test]
    fn find_from_on_lines_with_tabs_counts_tabs_as_graphemes() {
        let buf = make_buffer(&["a\tb c"]);

        assert_eq!(
            buf.find_from("c", Location { x: 0, y: 0 }),
            Some(Location { x: 4, y: 0 })
        );
    }

    #[test]
    fn find_from_without_a_match_is_none() {
        let buf = make_buffer(&["abc", "def"]);

        assert_eq!(buf.find_from("z", Location { x: 0, y: 0 }), None);
    }

    #[test]
    fn find_from_of_an_empty_pattern_is_none() {
        let buf = make_buffer(&["abc"]);

        assert_eq!(buf.find_from("", Location { x: 0, y: 0 }), None);
    }

    #[test]
    fn find_from_on_an_empty_buffer_is_none() {
        let buf = Buffer::default();

        assert_eq!(buf.find_from("a", Location { x: 0, y: 0 }), None);
    }

    #[test]
    fn find_wrapping_prefers_the_match_after_the_start_position() {
        let buf = make_buffer(&["hello world hello"]);

        // The forward match at 12 wins over the wrapped one at 0.
        assert_eq!(
            buf.find_wrapping("hello", Location { x: 6, y: 0 }),
            Some(Location { x: 12, y: 0 })
        );
    }

    #[test]
    fn find_wrapping_finds_a_match_that_starts_exactly_at_the_start_position() {
        let buf = make_buffer(&["abcabc"]);

        assert_eq!(
            buf.find_wrapping("abc", Location { x: 3, y: 0 }),
            Some(Location { x: 3, y: 0 })
        );
    }

    #[test]
    fn find_wrapping_wraps_to_the_top_when_nothing_matches_after_the_start() {
        let buf = make_buffer(&["hello world hello"]);

        // After column 13 only "ello" remains, so the search wraps.
        assert_eq!(
            buf.find_wrapping("hello", Location { x: 13, y: 0 }),
            Some(Location { x: 0, y: 0 })
        );
    }

    #[test]
    fn find_wrapping_from_a_later_line_wraps_to_an_earlier_line() {
        let buf = make_buffer(&["abc", "xyz", "abc"]);

        assert_eq!(
            buf.find_wrapping("abc", Location { x: 1, y: 2 }),
            Some(Location { x: 0, y: 0 })
        );
    }

    #[test]
    fn find_wrapping_from_a_position_past_the_end_of_the_buffer_wraps_to_the_top() {
        let buf = make_buffer(&["abc"]);

        // Row 1 is the "one past the end" position.
        assert_eq!(
            buf.find_wrapping("a", Location { x: 0, y: 1 }),
            Some(Location { x: 0, y: 0 })
        );
    }

    #[test]
    fn find_wrapping_of_a_pattern_that_matches_nowhere_is_none() {
        let buf = make_buffer(&["abc", "def"]);

        assert_eq!(buf.find_wrapping("z", Location { x: 1, y: 0 }), None);
    }

    #[test]
    fn find_wrapping_of_an_empty_pattern_is_none() {
        let buf = make_buffer(&["abc"]);

        assert_eq!(buf.find_wrapping("", Location { x: 0, y: 0 }), None);
    }

    #[test]
    fn find_wrapping_on_an_empty_buffer_is_none() {
        let buf = Buffer::default();

        assert_eq!(buf.find_wrapping("a", Location { x: 0, y: 0 }), None);
    }

    #[test]
    fn find_before_returns_the_last_match_ending_at_or_before_the_position() {
        let buf = make_buffer(&["hello world hello"]);

        // The last "hello" ends at column 17, past the position.
        assert_eq!(
            buf.find_before("hello", Location { x: 16, y: 0 }),
            Some(Location { x: 0, y: 0 })
        );
        // A match ending exactly at the position counts.
        assert_eq!(
            buf.find_before("hello", Location { x: 17, y: 0 }),
            Some(Location { x: 12, y: 0 })
        );
    }

    #[test]
    fn find_before_scans_the_preceding_lines_backwards() {
        let buf = make_buffer(&["hello world hello", "hello again"]);

        // Nothing on line 1 before column 2; the last match of line 0
        // is the closest one before the position.
        assert_eq!(
            buf.find_before("hello", Location { x: 2, y: 1 }),
            Some(Location { x: 12, y: 0 })
        );
    }

    #[test]
    fn find_before_at_the_top_of_the_buffer_is_none() {
        let buf = make_buffer(&["hello", "hello"]);

        assert_eq!(buf.find_before("hello", Location { x: 0, y: 0 }), None);
    }

    #[test]
    fn find_before_of_a_position_past_the_end_of_the_buffer_is_the_last_match() {
        let buf = make_buffer(&["hello", "world hello"]);

        // Row 2 is the "one past the end" position: every match sits
        // before it, so the buffer's last occurrence wins.
        assert_eq!(
            buf.find_before("hello", Location { x: 0, y: 2 }),
            Some(Location { x: 6, y: 1 })
        );
    }

    #[test]
    fn find_before_of_a_multibyte_pattern_counts_graphemes_not_bytes() {
        // 'é' is two bytes in UTF-8, so the second "café" starts at
        // grapheme 5 of "café café".
        let buf = make_buffer(&["café café"]);

        assert_eq!(
            buf.find_before("café", Location { x: 9, y: 0 }),
            Some(Location { x: 5, y: 0 })
        );
    }

    #[test]
    fn find_before_of_an_empty_pattern_is_none() {
        let buf = make_buffer(&["abc"]);

        assert_eq!(buf.find_before("", Location { x: 0, y: 0 }), None);
    }

    #[test]
    fn find_before_without_a_match_is_none() {
        let buf = make_buffer(&["abc", "def"]);

        assert_eq!(buf.find_before("z", Location { x: 0, y: 1 }), None);
    }

    #[test]
    fn find_wrapping_before_prefers_the_match_before_the_position() {
        let buf = make_buffer(&["hello world hello"]);

        // The match at 0 sits before the position; the one at 12 sits
        // after it and is only reachable by wrapping.
        assert_eq!(
            buf.find_wrapping_before("hello", Location { x: 14, y: 0 }),
            Some(Location { x: 0, y: 0 })
        );
    }

    #[test]
    fn find_wrapping_before_wraps_to_the_end_when_nothing_matches_before() {
        let buf = make_buffer(&["hello world hello"]);

        assert_eq!(
            buf.find_wrapping_before("hello", Location { x: 2, y: 0 }),
            Some(Location { x: 12, y: 0 })
        );
    }

    #[test]
    fn find_wrapping_before_wraps_to_the_end_of_a_later_line() {
        let buf = make_buffer(&["xyz", "abc"]);

        // Nothing matches before the position on any line: the search
        // wraps to the end of the buffer.
        assert_eq!(
            buf.find_wrapping_before("abc", Location { x: 0, y: 0 }),
            Some(Location { x: 0, y: 1 })
        );
    }

    #[test]
    fn find_wrapping_before_of_a_pattern_that_matches_nowhere_is_none() {
        let buf = make_buffer(&["abc", "def"]);

        assert_eq!(buf.find_wrapping_before("z", Location { x: 1, y: 0 }), None);
    }

    #[test]
    fn find_wrapping_before_of_an_empty_pattern_is_none() {
        let buf = make_buffer(&["abc"]);

        assert_eq!(buf.find_wrapping_before("", Location { x: 0, y: 0 }), None);
    }

    #[test]
    fn find_wrapping_before_on_an_empty_buffer_is_none() {
        let buf = Buffer::default();

        assert_eq!(buf.find_wrapping_before("a", Location { x: 0, y: 0 }), None);
    }

    #[test]
    fn save_writes_buffer_contents_to_file() {
        let path = std::env::temp_dir().join(format!("sted_test_save_{}.txt", std::process::id()));
        std::fs::write(&path, "line one\nline two\n").unwrap();
        let file_path = path.to_string_lossy().to_string();

        let mut buf = Buffer::load(&file_path).expect("load should succeed");
        buf.insert_char(Location { x: 0, y: 1 }, 'X');
        buf.save().expect("save should succeed");

        let saved = std::fs::read_to_string(&file_path).unwrap();
        let saved_lines: Vec<&str> = saved.lines().collect();
        assert_eq!(saved_lines, vec!["line one", "Xline two"]);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_without_file_name_is_an_error() {
        let mut buf = make_buffer(&["hello"]);
        assert!(buf.save().is_err());
    }
}
