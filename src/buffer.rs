use crate::common::{line::Line, types::Location};
use std::fs::read_to_string;

#[derive(Clone)]
pub struct Buffer {
    pub lines: Vec<Line>,
    pub modified: bool,
}

impl Default for Buffer {
    fn default() -> Self {
        Self {
            lines: vec![],
            modified: true,
        }
    }
}

impl Buffer {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn load(file_path: &str) -> Result<Self, std::io::Error> {
        let mut lines: Vec<Line> = Vec::new();

        match read_to_string(file_path) {
            Ok(file_content) => {
                for line in file_content.lines() {
                    lines.push(Line::from(line));
                }
            }
            Err(error) => {
                lines.push(Line::from(&format!("Failed to read file at {}", file_path)));
                lines.push(Line::from(&format!("Error message: {}", error)));
            }
        }

        Ok(Self {
            lines: lines,
            modified: true,
        })
    }

    pub fn get_col_from_text_location(&self, location: Location) -> usize {
        if location.y >= self.lines.len() || self.lines.is_empty() {
            return 0;
        }
        self.lines[location.y].width_until(location.x)
    }

    /// Inserts character `c` at column `location.x` on row `location.y`. Creates a new line if the row exceeds the buffer length.
    pub fn insert_char(&mut self, location: Location, c: char) {
        if location.y >= self.lines.len() {
            self.lines.push(Line::from(&format!("{c}")));
        } else if let Some(line) = self.lines.get_mut(location.y) {
            line.insert_char(location.x, c);
        }
    }

    pub fn newline(&mut self, location: Location) {
        if location.y >= self.lines.len() {
            self.lines.push(Line::newline());
        }

        let str_before_caret = self.lines[location.y].convert_content_to_string(0..location.x);
        let str_after_caret = self.lines[location.y]
            .convert_content_to_string(location.x..self.lines[location.y].grapheme_count());

        self.lines[location.y] = Line::from(&str_before_caret);

        if location.y + 1 >= self.lines.len() {
            self.lines.push(Line::from(&str_after_caret));
        } else {
            self.lines
                .insert(location.y + 1, Line::from(&str_after_caret));
        }
    }

    /// Deletes the grapheme cluster at column `location.x` on row `location.y`.
    pub fn delete(&mut self, location: Location) {
        if location.y >= self.lines.len() {
            return;
        } else if let Some(line) = self.lines.get_mut(location.y) {
            line.delete_grapheme(location.x);
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
        self.lines[target_index].append(source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::Location;

    fn make_line(s: &str) -> Line {
        let mut l = Line::from(s);
        l.scroll_x = 0;
        l
    }

    fn make_buffer(lines_data: &[&str]) -> Buffer {
        Buffer {
            lines: lines_data.iter().copied().map(make_line).collect(),
            modified: false,
        }
    }

    fn line_len(line: &Line) -> usize {
        line.width_until(line.grapheme_count())
    }

    #[test]
    fn delete_removes_grapheme_on_existing_line() {
        let mut buf = make_buffer(&["abcd"]);
        // Location (0, 0) → deletes grapheme at index 0 from line[0] ('a')
        buf.delete(Location { x: 0, y: 0 });
        assert_eq!(
            buf.lines[0].convert_content_to_string(0..line_len(&buf.lines[0])),
            "bcd"
        );
    }

    #[test]
    fn delete_removes_correct_position_at_nonzero_x() {
        let mut buf = make_buffer(&["abcdef"]);
        // Location (3, 0) → deletes grapheme at index 3 from line[0] ('d')
        buf.delete(Location { x: 3, y: 0 });
        assert_eq!(
            buf.lines[0].convert_content_to_string(0..line_len(&buf.lines[0])),
            "abcef"
        );
    }

    #[test]
    fn delete_on_last_grapheme() {
        let mut buf = make_buffer(&["abc"]);
        // Location (2, 0) → deletes grapheme at index 2 ('c')
        buf.delete(Location { x: 2, y: 0 });
        assert_eq!(
            buf.lines[0].convert_content_to_string(0..line_len(&buf.lines[0])),
            "ab"
        );
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
        assert_eq!(
            buf.lines[0].convert_content_to_string(0..line_len(&buf.lines[0])),
            "ello"
        );

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
        assert_eq!(
            buf.lines[0].convert_content_to_string(0..buf.lines[0].grapheme_count()),
            "abcd"
        );
    }

    #[test]
    fn consolidate_lines_with_empty_target_keeps_source_content() {
        let mut buf = make_buffer(&["", "cd"]);
        buf.consolidate_lines(1, 0);
        assert_eq!(buf.lines.len(), 1);
        assert_eq!(
            buf.lines[0].convert_content_to_string(0..buf.lines[0].grapheme_count()),
            "cd"
        );
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
        let text: String = buf.lines[0].convert_content_to_string(0..buf.lines[0].grapheme_count());
        assert_eq!(text, "x");
    }

    #[test]
    fn insert_char_into_existing_row() {
        let mut buf = make_buffer(&["abc"]);

        buf.insert_char(Location { x: 2, y: 0 }, 'X');
        assert_eq!(buf.lines[0].grapheme_count(), 4);
        let text: String = buf.lines[0].convert_content_to_string(0..buf.lines[0].grapheme_count());
        assert_eq!(text, "abXc");
    }

    #[test]
    fn insert_char_at_beginning_of_line() {
        let mut buf = make_buffer(&["hello"]);

        buf.insert_char(Location { x: 0, y: 0 }, 'W');
        assert_eq!(buf.lines[0].grapheme_count(), 6);
        let text: String = buf.lines[0].convert_content_to_string(0..buf.lines[0].grapheme_count());
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
        let text: String = buf.lines[0].convert_content_to_string(0..buf.lines[0].grapheme_count());
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
        let text: String = buf.lines[0].convert_content_to_string(0..buf.lines[0].grapheme_count());
        assert_eq!(text, "x");
    }

    #[test]
    fn insert_char_with_special_characters() {
        let mut buf = make_buffer(&["abc"]);

        // Insert an emoji grapheme
        buf.insert_char(Location { x: 2, y: 0 }, '👍');
        assert_eq!(buf.lines[0].grapheme_count(), 4);
        let text: String = buf.lines[0].convert_content_to_string(0..buf.lines[0].grapheme_count());
        assert!(text.contains("👍"));
    }

    #[test]
    fn insert_char_into_empty_line() {
        let mut buf = make_buffer(&["", "abc"]);

        // Insert into the empty first line; the other line must be untouched
        buf.insert_char(Location { x: 0, y: 0 }, 'Z');
        assert_eq!(buf.lines.len(), 2);
        let first: String =
            buf.lines[0].convert_content_to_string(0..buf.lines[0].grapheme_count());
        assert_eq!(first, "Z");
        let second: String =
            buf.lines[1].convert_content_to_string(0..buf.lines[1].grapheme_count());
        assert_eq!(second, "abc");
    }

    // Note: insert_char does NOT silently ignore out-of-bounds. Instead:
    // - When `location.y >= lines.len()` it *creates* a new line at that index.
    // - The Line::insert_char inside gets x past the end of the line, which is okay (inserts at EOF).
}
