use std::fs;
use std::path::{Path, PathBuf};

use tree_sitter::{Node, Parser, Tree};

use crate::error::{Error, Result};
use crate::lang::Lang;

#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub lang: Lang,
    pub text: String,
    pub tree: Tree,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn load(path: &Path) -> Result<Self> {
        let Some(lang) = Lang::from_path(path) else {
            return Err(Error::UnsupportedLanguage(path.to_path_buf()));
        };
        let bytes = fs::read(path).map_err(|err| Error::Io(path.to_path_buf(), err))?;
        let text = String::from_utf8(bytes).map_err(|_| Error::NotUtf8(path.to_path_buf()))?;
        Self::from_text(path, lang, text)
    }

    pub fn from_text(path: &Path, lang: Lang, text: String) -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&lang.ts_language())
            .map_err(|_| Error::Parse(path.to_path_buf()))?;
        let tree = parser
            .parse(&text, None)
            .ok_or_else(|| Error::Parse(path.to_path_buf()))?;
        let line_starts = compute_line_starts(&text);
        Ok(Self {
            path: path.to_path_buf(),
            lang,
            text,
            tree,
            line_starts,
        })
    }

    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    pub fn slice(&self, start: usize, end: usize) -> &str {
        let start = start.min(self.text.len());
        let end = end.clamp(start, self.text.len());
        &self.text[start..end]
    }

    pub fn node_text(&self, node: Node<'_>) -> &str {
        self.slice(node.start_byte(), node.end_byte())
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn line_of(&self, byte: usize) -> usize {
        self.line_starts.partition_point(|start| *start <= byte)
    }

    pub fn line_start_byte(&self, line: usize) -> usize {
        let index = line.saturating_sub(1);
        self.line_starts
            .get(index)
            .copied()
            .unwrap_or(self.text.len())
    }

    pub fn line_end_byte(&self, line: usize) -> usize {
        self.line_starts
            .get(line)
            .map_or(self.text.len(), |next| *next)
    }

    pub fn line_text(&self, line: usize) -> &str {
        let start = self.line_start_byte(line);
        let end = self.line_end_byte(line);
        self.slice(start, end).trim_end_matches(['\n', '\r'])
    }

    pub fn snap_to_line_start(&self, byte: usize) -> usize {
        let line = self.line_of(byte);
        let start = self.line_start_byte(line);
        if self.slice(start, byte).trim().is_empty() {
            start
        } else {
            byte
        }
    }

    pub fn extend_to_line_end(&self, byte: usize) -> usize {
        let line = self.line_of(byte.saturating_sub(1).max(1));
        let end = self.line_end_byte(line);
        end.max(byte)
    }

    pub fn lines_span(&self, start_line: usize, end_line: usize) -> (usize, usize) {
        let start = self.line_start_byte(start_line);
        let end = self.line_end_byte(end_line.min(self.line_count()));
        (start, end)
    }
}

fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    if starts.len() > 1 && starts[starts.len() - 1] == text.len() {
        starts.pop();
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SourceFile {
        SourceFile::from_text(
            Path::new("t.py"),
            Lang::Python,
            "a = 1\nb = 2\nc = 3\n".to_string(),
        )
        .expect("parses")
    }

    #[test]
    fn counts_lines() {
        assert_eq!(sample().line_count(), 3);
    }

    #[test]
    fn maps_bytes_to_lines() {
        let file = sample();
        assert_eq!(file.line_of(0), 1);
        assert_eq!(file.line_of(6), 2);
        assert_eq!(file.line_of(12), 3);
    }

    #[test]
    fn reads_line_text() {
        assert_eq!(sample().line_text(2), "b = 2");
    }
}
