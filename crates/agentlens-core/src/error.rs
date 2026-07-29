use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    AddressMissingHash(String),
    AddressEmptyPath(String),
    BadLineSpan(String),
    UnsupportedLanguage(PathBuf),
    Io(PathBuf, std::io::Error),
    NotUtf8(PathBuf),
    Parse(PathBuf),
    BadRegex(String),
    BadKind(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AddressMissingHash(raw) => write!(
                f,
                "address `{raw}` has no `#`: use `{raw}#Symbol`, or `{raw}#` for the outline"
            ),
            Self::AddressEmptyPath(raw) => write!(
                f,
                "address `{raw}` has no file part: bare-name lookup is not supported yet"
            ),
            Self::BadLineSpan(raw) => {
                write!(f, "bad line span `{raw}`: expected `L<start>-L<end>`")
            }
            Self::UnsupportedLanguage(path) => {
                write!(f, "unsupported language for `{}`", path.display())
            }
            Self::Io(path, err) => write!(f, "cannot read `{}`: {err}", path.display()),
            Self::NotUtf8(path) => write!(f, "`{}` is not valid utf-8", path.display()),
            Self::Parse(path) => write!(f, "cannot parse `{}`", path.display()),
            Self::BadRegex(pattern) => write!(f, "bad pattern `{pattern}`"),
            Self::BadKind(kind) => write!(f, "unknown kind `{kind}`"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(_, err) => Some(err),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
