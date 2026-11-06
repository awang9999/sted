use std::fmt;
use std::path::Path;

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct Location {
    pub x: usize,
    pub y: usize,
}

/// The kind of file the editor has open, detected from its name. Each
/// kind carries its own highlighting rules; plain text carries none.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub enum FileType {
    /// No file type detected: no highlighting rules apply.
    #[default]
    PlainText,
    /// A Rust source file: a name ending in `.rs`.
    Rust,
}

impl FileType {
    /// The type of the file at `path`, decided by its extension: a name
    /// whose extension is `rs`, in any case, is Rust; everything else —
    /// other extensions, no extension — is plain text.
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some(ext) if ext.eq_ignore_ascii_case("rs") => Self::Rust,
            _ => Self::PlainText,
        }
    }
}

impl fmt::Display for FileType {
    /// The short label the status bar shows for the type: `Text` for
    /// plain text, the language name for a detected language.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::PlainText => "Text",
            Self::Rust => "Rust",
        };
        f.write_str(label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_ending_in_rs_is_rust() {
        assert_eq!(FileType::from_path(Path::new("src/main.rs")), FileType::Rust);
    }

    #[test]
    fn the_rs_extension_matches_regardless_of_case() {
        assert_eq!(FileType::from_path(Path::new("MAIN.RS")), FileType::Rust);
        assert_eq!(FileType::from_path(Path::new("main.Rs")), FileType::Rust);
    }

    #[test]
    fn other_extensions_are_plain_text() {
        assert_eq!(
            FileType::from_path(Path::new("notes.txt")),
            FileType::PlainText
        );
        assert_eq!(
            FileType::from_path(Path::new("main.rs.bak")),
            FileType::PlainText
        );
    }

    #[test]
    fn a_name_without_an_extension_is_plain_text() {
        assert_eq!(FileType::from_path(Path::new("Makefile")), FileType::PlainText);
    }

    #[test]
    fn the_default_file_type_is_plain_text() {
        assert_eq!(FileType::default(), FileType::PlainText);
    }

    #[test]
    fn the_display_labels_are_text_and_rust() {
        assert_eq!(FileType::PlainText.to_string(), "Text");
        assert_eq!(FileType::Rust.to_string(), "Rust");
    }
}
