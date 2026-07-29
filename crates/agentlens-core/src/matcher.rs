use regex::Regex;

use crate::error::{Error, Result};

#[derive(Debug)]
pub enum Matcher {
    Exact(String),
    Pattern(Regex),
}

impl Matcher {
    /// Build a matcher for `pattern`, literal when `exact` is set and a regex otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`Error::BadRegex`] if `exact` is false and `pattern` is not a valid regex.
    pub fn new(pattern: &str, exact: bool) -> Result<Self> {
        if exact {
            return Ok(Self::Exact(pattern.to_string()));
        }
        Regex::new(pattern)
            .map(Self::Pattern)
            .map_err(|_| Error::BadRegex(pattern.to_string()))
    }

    pub fn matches(&self, text: &str) -> bool {
        match self {
            Self::Exact(wanted) => wanted == text,
            Self::Pattern(regex) => regex.is_match(text),
        }
    }

    pub fn matches_whole(&self, text: &str) -> bool {
        match self {
            Self::Exact(wanted) => wanted == text,
            Self::Pattern(regex) => regex
                .find(text)
                .is_some_and(|found| found.start() == 0 && found.end() == text.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_is_whole_symbol() {
        let matcher = Matcher::new("create_user", true).expect("built");
        assert!(matcher.matches("create_user"));
        assert!(!matcher.matches("create_user_v2"));
    }

    #[test]
    fn regex_is_substring_by_default() {
        let matcher = Matcher::new("create", false).expect("built");
        assert!(matcher.matches("create_user"));
        assert!(!matcher.matches_whole("create_user"));
    }

    #[test]
    fn rejects_bad_regex() {
        assert!(Matcher::new("(", false).is_err());
    }
}
