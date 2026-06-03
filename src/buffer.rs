pub struct Buffer {
    pub lines: Vec<String>,
}

impl Buffer {
    pub fn default() -> Self {
        let mut lines: Vec<String> = Vec::new();
        lines.push(String::from("Hello, World!"));

        Buffer { lines: lines }
    }
}
