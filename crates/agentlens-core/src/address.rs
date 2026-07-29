use std::fmt;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    Outline,
    Symbol(Vec<String>),
    Lines(usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub path: PathBuf,
    pub selector: Selector,
}

impl Address {
    /// Parse a `path#selector` address.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AddressMissingHash`] if `raw` has no `#`,
    /// [`Error::AddressEmptyPath`] if the path before `#` is empty, and
    /// [`Error::BadLineSpan`] if a line-span selector is malformed.
    pub fn parse(raw: &str) -> Result<Self> {
        let Some(hash) = raw.find('#') else {
            return Err(Error::AddressMissingHash(raw.to_string()));
        };
        let (path_part, symbol_part) = raw.split_at(hash);
        let symbol_part = &symbol_part[1..];
        if path_part.is_empty() {
            return Err(Error::AddressEmptyPath(raw.to_string()));
        }
        let selector = parse_selector(symbol_part, raw)?;
        Ok(Self {
            path: PathBuf::from(path_part),
            selector,
        })
    }

    pub fn outline(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            selector: Selector::Outline,
        }
    }

    pub fn symbol(path: &Path, dotted: &str) -> Self {
        Self {
            path: path.to_path_buf(),
            selector: Selector::Symbol(dotted.split('.').map(str::to_string).collect()),
        }
    }

    pub fn dotted(&self) -> String {
        match &self.selector {
            Selector::Symbol(parts) => parts.join("."),
            Selector::Outline => String::new(),
            Selector::Lines(start, end) => format!("L{start}-L{end}"),
        }
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}#{}",
            crate::render::slash_path(&self.path),
            self.dotted()
        )
    }
}

fn parse_selector(symbol_part: &str, raw: &str) -> Result<Selector> {
    if symbol_part.is_empty() {
        return Ok(Selector::Outline);
    }
    if looks_like_line_span(symbol_part) {
        return parse_line_span(symbol_part, raw);
    }
    let parts: Vec<String> = symbol_part.split('.').map(str::to_string).collect();
    if parts.iter().any(String::is_empty) {
        return Err(Error::AddressEmptyPath(raw.to_string()));
    }
    Ok(Selector::Symbol(parts))
}

fn looks_like_line_span(symbol_part: &str) -> bool {
    let mut chars = symbol_part.chars();
    if chars.next() != Some('L') {
        return false;
    }
    chars.next().is_some_and(|c| c.is_ascii_digit())
}

fn parse_line_span(symbol_part: &str, raw: &str) -> Result<Selector> {
    let bad = || Error::BadLineSpan(raw.to_string());
    let rest = symbol_part.strip_prefix('L').ok_or_else(bad)?;
    let (start, end) = match rest.split_once("-L") {
        Some((start, end)) => (start, end),
        None => (rest, rest),
    };
    let start: usize = start.parse().map_err(|_| bad())?;
    let end: usize = end.parse().map_err(|_| bad())?;
    if start == 0 || end < start {
        return Err(bad());
    }
    Ok(Selector::Lines(start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_outline() {
        let address = Address::parse("src/api/users.py#").expect("parses");
        assert_eq!(address.selector, Selector::Outline);
        assert_eq!(address.path, PathBuf::from("src/api/users.py"));
    }

    #[test]
    fn parses_dotted_symbol() {
        let address = Address::parse("a.py#Outer.Inner.method").expect("parses");
        assert_eq!(
            address.selector,
            Selector::Symbol(vec![
                "Outer".to_string(),
                "Inner".to_string(),
                "method".to_string()
            ])
        );
    }

    #[test]
    fn parses_line_span() {
        let address = Address::parse("a.py#L10-L20").expect("parses");
        assert_eq!(address.selector, Selector::Lines(10, 20));
    }

    #[test]
    fn parses_single_line_span() {
        let address = Address::parse("a.py#L7").expect("parses");
        assert_eq!(address.selector, Selector::Lines(7, 7));
    }

    #[test]
    fn rejects_missing_hash() {
        assert!(Address::parse("a.py").is_err());
    }

    #[test]
    fn rejects_inverted_span() {
        assert!(Address::parse("a.py#L20-L10").is_err());
    }

    #[test]
    fn symbol_named_like_a_line_is_a_symbol() {
        let address = Address::parse("a.py#Loader").expect("parses");
        assert_eq!(
            address.selector,
            Selector::Symbol(vec!["Loader".to_string()])
        );
    }
}
