use serde::Serialize;

pub const DEFAULT_BUDGET: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Detail {
    Full,
    Summary,
    Counts,
}

impl Detail {
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Full => Some(Self::Summary),
            Self::Summary => Some(Self::Counts),
            Self::Counts => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Summary => "summary",
            Self::Counts => "counts",
        }
    }

    pub fn ladder() -> [Self; 3] {
        [Self::Full, Self::Summary, Self::Counts]
    }
}

pub fn estimate_tokens(text: &str) -> usize {
    let mut units = 0usize;
    let mut in_word = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            if !in_word {
                units += 1;
                in_word = true;
            }
        } else {
            in_word = false;
            if !ch.is_whitespace() {
                units += 1;
            }
        }
    }
    let by_chars = text.chars().count().div_ceil(4);
    units.max(by_chars)
}

pub fn fit<T, F>(budget: usize, mut render: F) -> (String, Detail, bool)
where
    F: FnMut(Detail) -> T,
    T: AsRef<str>,
{
    let mut last = String::new();
    let mut last_detail = Detail::Full;
    for detail in Detail::ladder() {
        let text = render(detail).as_ref().to_string();
        let cost = estimate_tokens(&text);
        if cost <= budget {
            return (text, detail, detail != Detail::Full);
        }
        last = text;
        last_detail = detail;
    }
    (last, last_detail, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_free() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_grows_with_text() {
        let small = estimate_tokens("def f(): pass");
        let large = estimate_tokens(&"def f(): pass\n".repeat(50));
        assert!(large > small);
    }

    #[test]
    fn ladder_degrades_when_over_budget() {
        let (text, detail, degraded) = fit(3, |detail| match detail {
            Detail::Full => "a very long body that will not fit at all".to_string(),
            Detail::Summary => "still too long for three tokens".to_string(),
            Detail::Counts => "1 item".to_string(),
        });
        assert_eq!(detail, Detail::Counts);
        assert!(degraded);
        assert_eq!(text, "1 item");
    }

    #[test]
    fn ladder_keeps_full_when_it_fits() {
        let (_, detail, degraded) = fit(4000, |_| "short".to_string());
        assert_eq!(detail, Detail::Full);
        assert!(!degraded);
    }
}
