//! Errors, and the exit code each one carries.
//!
//! The contract agents branch on is three-valued: **0** the thing was found,
//! **1** it was not, **2** the tool itself failed. A mistyped address is not a
//! tool fault — it is a miss — so every address variant exits 1, as does a
//! path that is simply not on disk and a file whose language or format this
//! tool does not read. Exit 2 is reserved for a genuine fault: unreadable
//! bytes, invalid utf-8, a parse that should have worked, a caller flag value
//! the tool cannot act on at all.
//!
//! [`Error::exit_code`] is the single place that mapping lives.

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    AddressMissingHash(String),
    AddressEmptyPath(String),
    AddressUnresolved(String),
    BadLineSpan(String),
    UnsupportedLanguage(PathBuf),
    UnsupportedFormat(PathBuf),
    Io(PathBuf, std::io::Error),
    NotUtf8(PathBuf),
    Parse(PathBuf),
    BadRegex(String),
    BadKind {
        value: String,
        allowed: &'static str,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AddressMissingHash(raw) => write!(
                f,
                "no `#` in `{raw}`\n\
                 an address names a file and something in it, like `file#Name`"
            ),
            Self::AddressEmptyPath(raw) => write!(f, "no file part in `{raw}`"),
            Self::AddressUnresolved(raw) => write!(
                f,
                "cannot read `{raw}` as an address: no file on disk matches its path"
            ),
            Self::BadLineSpan(raw) => write!(
                f,
                "bad line span in `{raw}`: a span is `L10-L20`, counting from 1"
            ),
            Self::UnsupportedLanguage(path) => {
                write!(f, "unsupported language for `{}`", path.display())
            }
            Self::UnsupportedFormat(path) => write!(
                f,
                "unsupported format for `{}`: doclens reads json, yaml and markdown",
                path.display()
            ),
            Self::Io(path, err) => {
                write!(
                    f,
                    "cannot read `{}`: {}",
                    path.display(),
                    stable_reason(err)
                )
            }
            Self::NotUtf8(path) => write!(f, "`{}` is not valid utf-8", path.display()),
            Self::Parse(path) => write!(f, "cannot parse `{}`", path.display()),
            Self::BadRegex(pattern) => write!(f, "bad pattern `{pattern}`"),
            Self::BadKind { value, allowed } => {
                write!(f, "unknown value `{value}`: use {allowed}")
            }
        }
    }
}

impl Error {
    /// Whether this error is about the shape of an address.
    ///
    /// Each binary has its own address grammar and its own help topic, so the
    /// pointer to that help is added by the CLI rather than baked in here.
    pub fn is_address(&self) -> bool {
        matches!(
            self,
            Self::AddressMissingHash(_)
                | Self::AddressEmptyPath(_)
                | Self::AddressUnresolved(_)
                | Self::BadLineSpan(_)
        )
    }

    /// The process exit code this error carries: 1 for a miss, 2 for a fault.
    ///
    /// See the module docs for the reasoning behind each classification.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::AddressMissingHash(_)
            | Self::AddressEmptyPath(_)
            | Self::AddressUnresolved(_)
            | Self::BadLineSpan(_)
            | Self::UnsupportedLanguage(_)
            | Self::UnsupportedFormat(_) => 1,
            Self::Io(_, err) if err.kind() == std::io::ErrorKind::NotFound => 1,
            Self::Io(..)
            | Self::NotUtf8(_)
            | Self::Parse(_)
            | Self::BadRegex(_)
            | Self::BadKind { .. } => 2,
        }
    }
}

// The OS error string differs per platform ("No such file or directory" on
// Unix, "The system cannot find the file specified." on Windows). Snapshot
// output must be byte-identical across the CI matrix, so map the kind to
// stable text instead of rendering the raw error.
fn stable_reason(err: &std::io::Error) -> &'static str {
    use std::io::ErrorKind;
    match err.kind() {
        ErrorKind::NotFound => "not found",
        ErrorKind::PermissionDenied => "permission denied",
        ErrorKind::IsADirectory => "is a directory",
        _ => "read failed",
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
