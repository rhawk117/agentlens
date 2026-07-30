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

/// A record that the caller's arguments were rewritten into a real address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coercion {
    /// The canonical address the caller should have typed.
    pub read_as: String,
    /// What they typed instead, with any trailing positionals rejoined.
    pub raw: String,
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

    /// Parse an address the way a caller typed it, rewriting common near-misses.
    ///
    /// `raw` is the first positional argument and `extra` any that followed it,
    /// so `slice f.py 1 120` arrives as `("f.py", ["1", "120"])`. A rewrite
    /// happens only when the resulting file is on disk; nothing is guessed. On
    /// a rewrite the returned [`Coercion`] carries the address the caller
    /// should have typed, for the CLI to echo as a note.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AddressUnresolved`] when `extra` cannot be grounded on
    /// a real file, and otherwise whatever [`Address::parse`] returns.
    pub fn parse_lenient(raw: &str, extra: &[String]) -> Result<(Self, Option<Coercion>)> {
        let strict = Self::parse(raw);
        if extra.is_empty()
            && let Ok(address) = &strict
            && address.path.is_file()
        {
            return Ok((address.clone(), None));
        }
        if let Some(address) = coerce(raw, extra) {
            let coercion = Coercion {
                read_as: address.to_string(),
                raw: joined(raw, extra),
            };
            return Ok((address, Some(coercion)));
        }
        if !extra.is_empty() {
            return Err(Error::AddressUnresolved(joined(raw, extra)));
        }
        strict.map(|address| (address, None))
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

fn joined(raw: &str, extra: &[String]) -> String {
    if extra.is_empty() {
        return raw.to_string();
    }
    format!("{raw} {}", extra.join(" "))
}

// Every rule below ends in an is_file() check, so a rewrite can only ever
// produce an address that names something real. A typo stays a typo.
fn coerce(raw: &str, extra: &[String]) -> Option<Address> {
    if extra.is_empty() {
        return coerce_single(raw);
    }
    coerce_positional(raw, extra)
}

fn coerce_positional(raw: &str, extra: &[String]) -> Option<Address> {
    let path = Path::new(raw);
    if !path.is_file() {
        return None;
    }
    let selector = match extra {
        [only] => loose_selector(only)?,
        [start, end] => span(start, end)?,
        _ => return None,
    };
    Some(Address {
        path: path.to_path_buf(),
        selector,
    })
}

fn coerce_single(raw: &str) -> Option<Address> {
    let (head, tail) = match raw.split_once('#') {
        Some((head, tail)) => (head, Some(tail)),
        None => (raw, None),
    };
    if tail.is_none() && Path::new(head).is_file() {
        return Some(Address::outline(Path::new(head)));
    }
    let (path, rest) = split_at_file(head)?;
    let selector = colon_selector(rest, tail)?;
    Some(Address { path, selector })
}

// Walk the colons left to right and stop at the first prefix that is a real
// file, so a Windows drive letter cannot be mistaken for a separator.
fn split_at_file(head: &str) -> Option<(PathBuf, &str)> {
    for (index, _) in head.match_indices(':') {
        let candidate = head.get(..index)?;
        let path = Path::new(candidate);
        if path.is_file() {
            return Some((path.to_path_buf(), head.get(index..)?));
        }
    }
    None
}

// `F:1-140#Widget` and `F::MIDDLEWARE#Symbol` both carry two selectors. Prefer
// whichever one names a symbol: a line range next to a symbol is redundant,
// and the symbol is the address that survives an edit.
fn colon_selector(rest: &str, tail: Option<&str>) -> Option<Selector> {
    let trimmed = rest.trim_start_matches(':');
    let left = (!trimmed.is_empty())
        .then(|| loose_selector(trimmed))
        .flatten();
    let right = tail.and_then(|text| parse_selector(text, text).ok());
    match (left, right) {
        (Some(Selector::Symbol(parts)), _) => Some(Selector::Symbol(parts)),
        (_, Some(selector)) | (Some(selector), None) => Some(selector),
        (None, None) => None,
    }
}

fn loose_selector(text: &str) -> Option<Selector> {
    if let Some((start, end)) = text.split_once('-')
        && let Some(selector) = span(start, end)
    {
        return Some(selector);
    }
    if let Some(line) = line_number(text) {
        return Some(Selector::Lines(line, line));
    }
    bare_symbol(text)
}

fn span(start: &str, end: &str) -> Option<Selector> {
    let start = line_number(start)?;
    let end = line_number(end)?;
    (end >= start).then_some(Selector::Lines(start, end))
}

fn line_number(text: &str) -> Option<usize> {
    let value: usize = text.parse().ok()?;
    (value > 0).then_some(value)
}

fn bare_symbol(text: &str) -> Option<Selector> {
    if text.is_empty() || text.contains(char::is_whitespace) || text.contains('#') {
        return None;
    }
    let parts: Vec<String> = text.split('.').map(str::to_string).collect();
    if parts.iter().any(String::is_empty) {
        return None;
    }
    Some(Selector::Symbol(parts))
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

    struct Scratch {
        dir: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("agentlens-lenient-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("mkdir");
            std::fs::write(dir.join("m.py"), "x = 1\n").expect("write");
            Self { dir }
        }

        fn file(&self) -> String {
            self.dir.join("m.py").to_string_lossy().replace('\\', "/")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn lenient(raw: &str, extra: &[&str]) -> (Address, Option<Coercion>) {
        let extra: Vec<String> = extra.iter().map(|part| (*part).to_string()).collect();
        Address::parse_lenient(raw, &extra).expect("coerces")
    }

    #[test]
    fn lenient_coerces_positional_line_range() {
        let scratch = Scratch::new("positional-range");
        let (address, coercion) = lenient(&scratch.file(), &["1", "120"]);
        assert_eq!(address.selector, Selector::Lines(1, 120));
        assert_eq!(
            coercion.expect("noted").read_as,
            format!("{}#L1-L120", scratch.file())
        );
    }

    #[test]
    fn lenient_coerces_bare_file_to_an_outline() {
        let scratch = Scratch::new("bare-file");
        let (address, coercion) = lenient(&scratch.file(), &[]);
        assert_eq!(address.selector, Selector::Outline);
        assert!(coercion.is_some());
    }

    #[test]
    fn lenient_coerces_positional_symbol() {
        let scratch = Scratch::new("positional-symbol");
        let (address, _) = lenient(&scratch.file(), &["MIDDLEWARE"]);
        assert_eq!(
            address.selector,
            Selector::Symbol(vec!["MIDDLEWARE".into()])
        );
    }

    #[test]
    fn lenient_coerces_colon_line_range() {
        let scratch = Scratch::new("colon-range");
        let (address, _) = lenient(&format!("{}:1-140", scratch.file()), &[]);
        assert_eq!(address.selector, Selector::Lines(1, 140));
    }

    #[test]
    fn lenient_prefers_the_symbol_over_a_redundant_line_range() {
        let scratch = Scratch::new("range-and-symbol");
        let (address, _) = lenient(&format!("{}:1-140#Widget", scratch.file()), &[]);
        assert_eq!(address.selector, Selector::Symbol(vec!["Widget".into()]));
        assert_eq!(address.path, PathBuf::from(scratch.file()));
    }

    #[test]
    fn lenient_coerces_double_colon() {
        let scratch = Scratch::new("double-colon");
        let (address, _) = lenient(&format!("{}::MIDDLEWARE", scratch.file()), &[]);
        assert_eq!(
            address.selector,
            Selector::Symbol(vec!["MIDDLEWARE".into()])
        );
    }

    #[test]
    fn a_symbol_left_of_the_hash_beats_a_placeholder_right_of_it() {
        let scratch = Scratch::new("colon-symbol-hash");
        let (address, _) = lenient(&format!("{}::MIDDLEWARE#Symbol", scratch.file()), &[]);
        assert_eq!(
            address.selector,
            Selector::Symbol(vec!["MIDDLEWARE".into()])
        );
    }

    #[test]
    fn an_empty_selector_after_a_colon_range_is_an_outline() {
        let scratch = Scratch::new("colon-range-hash");
        let (address, _) = lenient(&format!("{}:300-390#", scratch.file()), &[]);
        assert_eq!(address.selector, Selector::Outline);
    }

    #[test]
    fn lenient_does_not_guess_when_the_path_is_absent() {
        let (address, coercion) =
            Address::parse_lenient("nowhere/absent.py#Thing", &[]).expect("strict parse survives");
        assert_eq!(address.selector, Selector::Symbol(vec!["Thing".into()]));
        assert!(coercion.is_none());
    }

    #[test]
    fn lenient_refuses_extra_arguments_it_cannot_ground_on_disk() {
        let error = Address::parse_lenient("absent.py", &["1".to_string(), "120".to_string()])
            .expect_err("no file, no coercion");
        assert!(matches!(error, Error::AddressUnresolved(_)));
    }

    #[test]
    fn a_half_numeric_range_is_not_a_range() {
        let scratch = Scratch::new("half-numeric");
        let error = Address::parse_lenient(&scratch.file(), &["1".to_string(), "x".to_string()])
            .expect_err("ambiguous");
        assert!(matches!(error, Error::AddressUnresolved(_)));
    }
}
