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

    // Inserts a character at the caret position. Moves the caret one to the right
    // If the caret is at the bottom of the doc, create a new line and then add the character.
    pub fn insert(&mut self, location: Location, c: char) {
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
            line.delete_grapheme(location.x + 1);
        }
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
        // Location (0, 0) → deletes grapheme at index 1 from line[0] ('b')
        buf.delete(Location { x: 0, y: 0 });
        assert_eq!(buf.lines[0].convert_content_to_string(0..line_len(&buf.lines[0])), "acd");
    }

    #[test]
    fn delete_removes_correct_position_at_nonzero_x() {
        let mut buf = make_buffer(&["abcdef"]);
        // Location (2, 0) → deletes grapheme at index 3 from line[0] ('d')
        buf.delete(Location { x: 2, y: 0 });
        assert_eq!(buf.lines[0].convert_content_to_string(0..line_len(&buf.lines[0])), "abcef");
    }

    #[test]
    fn delete_on_last_grapheme() {
        let mut buf = make_buffer(&["abc"]);
        // Location (1, 0) → deletes grapheme at index 2 ('c')
        buf.delete(Location { x: 1, y: 0 });
        assert_eq!(buf.lines[0].convert_content_to_string(0..line_len(&buf.lines[0])), "ab");
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
        assert_eq!(buf.lines[0].convert_content_to_string(0..line_len(&buf.lines[0])), "hllo");

        let second_before = buf.lines[1].grapheme_count();
        buf.delete(Location { x: 2, y: 1 }); // deletes from second line
        assert_eq!(buf.lines[1].grapheme_count(), second_before - 1);
    }
}
