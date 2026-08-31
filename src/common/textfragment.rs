#[derive(Clone)]
pub enum GraphemeWidth {
    Half,
    Full,
}

#[derive(Clone)]
pub struct TextFragment {
    pub grapheme: String,
    pub rendered_width: GraphemeWidth,
    pub replacement: Option<char>,
}

impl TextFragment {
    pub fn from(grapheme: &str, rendered_width: GraphemeWidth, replacement: Option<char>) -> Self {
        Self {
            grapheme: grapheme.to_string(),
            rendered_width: rendered_width,
            replacement: replacement,
        }
    }
}
