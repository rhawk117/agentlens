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
    /// Several addresses where the command takes exactly one.
    MultipleAddresses(Vec<String>),
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
            Self::MultipleAddresses(addresses) => write!(
                f,
                "this command takes one address, but got {}: {}\n\
                 run each address in its own call",
                addresses.len(),
                quoted_list(addresses)
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
    /// A stable machine-readable name for this error.
    ///
    /// Consumers branch on this rather than on the rendered message, which is
    /// free to change wording.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::AddressMissingHash(_) => "address_missing_hash",
            Self::AddressEmptyPath(_) => "address_empty_path",
            Self::AddressUnresolved(_) => "address_unresolved",
            Self::MultipleAddresses(_) => "multiple_addresses",
            Self::BadLineSpan(_) => "bad_line_span",
            Self::UnsupportedLanguage(_) => "unsupported_language",
            Self::UnsupportedFormat(_) => "unsupported_format",
            Self::Io(..) => "io",
            Self::NotUtf8(_) => "not_utf8",
            Self::Parse(_) => "parse",
            Self::BadRegex(_) => "bad_regex",
            Self::BadKind { .. } => "bad_value",
        }
    }

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
            | Self::MultipleAddresses(_)
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

fn quoted_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("`{item}`"))
        .collect::<Vec<_>>()
        .join(", ")
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract, restated independently of [`Error::exit_code`].
    ///
    /// This match has no wildcard arm on purpose: a new variant does not
    /// compile until somebody decides whether it is a miss or a fault.
    fn expected(error: &Error) -> u8 {
        match error {
            Error::AddressMissingHash(_)
            | Error::AddressEmptyPath(_)
            | Error::AddressUnresolved(_)
            | Error::MultipleAddresses(_)
            | Error::BadLineSpan(_)
            | Error::UnsupportedLanguage(_)
            | Error::UnsupportedFormat(_) => 1,
            Error::Io(_, err) if err.kind() == std::io::ErrorKind::NotFound => 1,
            Error::Io(..)
            | Error::NotUtf8(_)
            | Error::Parse(_)
            | Error::BadRegex(_)
            | Error::BadKind { .. } => 2,
        }
    }

    fn io(kind: std::io::ErrorKind) -> Error {
        Error::Io(PathBuf::from("f.py"), std::io::Error::from(kind))
    }

    fn one_of_every_variant() -> Vec<Error> {
        vec![
            Error::AddressMissingHash("raw".to_string()),
            Error::AddressEmptyPath("raw".to_string()),
            Error::AddressUnresolved("raw".to_string()),
            Error::MultipleAddresses(vec!["a.py#A".to_string(), "b.py#B".to_string()]),
            Error::BadLineSpan("raw".to_string()),
            Error::UnsupportedLanguage(PathBuf::from("f.rb")),
            Error::UnsupportedFormat(PathBuf::from("f.txt")),
            io(std::io::ErrorKind::NotFound),
            io(std::io::ErrorKind::PermissionDenied),
            Error::NotUtf8(PathBuf::from("f.py")),
            Error::Parse(PathBuf::from("f.py")),
            Error::BadRegex("[".to_string()),
            Error::BadKind {
                value: "nope".to_string(),
                allowed: "a or b",
            },
        ]
    }

    #[test]
    fn exit_codes() {
        for error in one_of_every_variant() {
            assert_eq!(
                error.exit_code(),
                expected(&error),
                "{} is classified inconsistently",
                error.kind()
            );
        }
    }

    #[test]
    fn every_variant_is_covered_by_the_exit_code_test() {
        let kinds: std::collections::BTreeSet<&str> =
            one_of_every_variant().iter().map(Error::kind).collect();
        assert_eq!(
            kinds,
            [
                "address_empty_path",
                "address_missing_hash",
                "address_unresolved",
                "bad_line_span",
                "bad_regex",
                "bad_value",
                "io",
                "multiple_addresses",
                "not_utf8",
                "parse",
                "unsupported_format",
                "unsupported_language",
            ]
            .into_iter()
            .collect(),
            "a variant was added or renamed without updating the exit-code test"
        );
    }

    #[test]
    fn a_missing_file_is_a_miss_but_an_unreadable_one_is_a_fault() {
        assert_eq!(io(std::io::ErrorKind::NotFound).exit_code(), 1);
        assert_eq!(io(std::io::ErrorKind::PermissionDenied).exit_code(), 2);
    }

    #[test]
    fn no_error_exits_zero() {
        for error in one_of_every_variant() {
            assert_ne!(error.exit_code(), 0, "{} claims success", error.kind());
        }
    }
}
