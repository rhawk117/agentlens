use std::path::Path;

use tree_sitter::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Lang {
    Python,
}

impl Lang {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        match ext {
            "py" | "pyi" => Some(Self::Python),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Python => "python",
        }
    }

    pub fn ts_language(self) -> Language {
        match self {
            Self::Python => tree_sitter_python::LANGUAGE.into(),
        }
    }

    pub fn comment_kinds(self) -> &'static [&'static str] {
        match self {
            Self::Python => &["comment"],
        }
    }

    pub fn string_kinds(self) -> &'static [&'static str] {
        match self {
            Self::Python => &["string", "concatenated_string", "string_content"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_python() {
        assert_eq!(Lang::from_path(Path::new("a/b.py")), Some(Lang::Python));
        assert_eq!(Lang::from_path(Path::new("a/b.pyi")), Some(Lang::Python));
        assert_eq!(Lang::from_path(Path::new("a/b.rs")), None);
    }
}
