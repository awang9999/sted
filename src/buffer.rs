use std::fs::read_to_string;

#[derive(Clone)]
pub struct Buffer {
    pub lines: Vec<String>,
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
        let mut my_lines: Vec<String> = Vec::new();

        match read_to_string(file_path) {
            Ok(file_content) => {
                for line in file_content.lines() {
                    my_lines.push(String::from(line));
                }
            }
            Err(error) => {
                my_lines.push(String::from(format!(
                    "Failed to read file at {}",
                    file_path
                )));
                my_lines.push(String::from(format!("Error message: {}", error)));
            }
        }

        Ok(Self {
            lines: my_lines,
            modified: true,
        })
    }
}
