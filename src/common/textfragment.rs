#[derive(Clone)]
pub enum GraphemeWidth {
    Half,
    Full,
}

#[derive(Clone)]
pub struct TextFragment {
    pub grapheme: String,
    pub rendered_width: GraphemeWidth,
    pub replacement: Option<String>,
}

impl TextFragment {
    pub fn from(
        grapheme: &str,
        rendered_width: GraphemeWidth,
        replacement: Option<String>,
    ) -> Self {
        Self {
            grapheme: grapheme.to_string(),
            rendered_width: rendered_width,
            replacement: replacement,
        }
    }

    pub fn get_display_text(&self) -> &str {
        match &self.replacement {
            Some(replacement) => replacement.as_str(),
            None => &self.grapheme,
        }
    }
}
