use std::path::Path;

pub fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn commas(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let lead = digits.len() % 3;
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && index % 3 == lead {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn plural(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("{} {singular}", commas(count))
    } else {
        format!("{} {plural}", commas(count))
    }
}

pub fn pad(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        text.to_string()
    } else {
        format!("{text}{}", " ".repeat(width - len))
    }
}

pub fn line_span(start: usize, end: usize) -> String {
    if start == end {
        format!("L{start}")
    } else {
        format!("L{start}-L{end}")
    }
}

pub fn indent(level: usize) -> String {
    "  ".repeat(level)
}

#[derive(Debug, Default)]
pub struct Lines {
    buffer: Vec<String>,
}

impl Lines {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, line: impl Into<String>) {
        self.buffer.push(line.into());
    }

    pub fn blank(&mut self) {
        if self.buffer.last().is_some_and(String::is_empty) {
            return;
        }
        if self.buffer.is_empty() {
            return;
        }
        self.buffer.push(String::new());
    }

    pub fn extend_block(&mut self, block: &str) {
        for line in block.split('\n') {
            self.buffer.push(line.to_string());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn finish(mut self) -> String {
        while self.buffer.last().is_some_and(String::is_empty) {
            self.buffer.pop();
        }
        if self.buffer.is_empty() {
            return String::new();
        }
        let mut out = self.buffer.join("\n");
        out.push('\n');
        out
    }
}

pub fn truncation_note(shown: usize, total: usize, more: &str) -> Option<String> {
    if shown >= total {
        return None;
    }
    Some(format!(
        "showing {} of {} — {more}",
        commas(shown),
        commas(total)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands() {
        assert_eq!(commas(0), "0");
        assert_eq!(commas(999), "999");
        assert_eq!(commas(1000), "1,000");
        assert_eq!(commas(6110), "6,110");
        assert_eq!(commas(1_234_567), "1,234,567");
    }

    #[test]
    fn collapses_multiline_signatures() {
        assert_eq!(collapse_ws("def f(a,\n   b):"), "def f(a, b):");
    }

    #[test]
    fn renders_spans() {
        assert_eq!(line_span(3, 3), "L3");
        assert_eq!(line_span(3, 9), "L3-L9");
    }

    #[test]
    fn notes_truncation_only_when_cut() {
        assert!(truncation_note(50, 50, "x").is_none());
        assert_eq!(
            truncation_note(50, 400, "raise --budget").expect("note"),
            "showing 50 of 400 — raise --budget"
        );
    }
}
