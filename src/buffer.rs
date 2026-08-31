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
}
