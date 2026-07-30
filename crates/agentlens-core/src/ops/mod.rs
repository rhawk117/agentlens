pub mod callers;
pub mod dead;
pub mod doc;
pub mod find;
pub mod literals;
pub mod map;
pub mod packet;
pub mod slice;
pub mod sym;

use serde_json::{Value, json};

use crate::index::IndexStats;

/// Version of the JSON envelope shape, bumped when a consumer would have to
/// change to keep reading it. Field additions do not bump it; removals and
/// renames do.
pub const SCHEMA_VERSION: u64 = 1;

#[derive(Debug)]
pub struct Report {
    pub text: String,
    pub json: Value,
    pub found: bool,
    /// Index freshness, for the commands that build one.
    pub index: Option<IndexStats>,
}

impl Report {
    pub fn new(text: String, json: Value, found: bool) -> Self {
        Self {
            text,
            json,
            found,
            index: None,
        }
    }

    #[must_use]
    pub fn indexed(mut self, stats: IndexStats) -> Self {
        self.index = Some(stats);
        self
    }
}

/// Stamp the fields every envelope carries onto one op's own JSON.
///
/// Each op builds the body it knows about; the shared keys are added here so
/// they cannot drift per command.
pub fn envelope(report: &Report, tool_version: &str) -> Value {
    let mut value = report.json.clone();
    let Some(fields) = value.as_object_mut() else {
        return value;
    };
    fields.insert("schema_version".to_string(), json!(SCHEMA_VERSION));
    fields.insert("tool_version".to_string(), json!(tool_version));
    if let Some(stats) = report.index {
        fields.insert(
            "index".to_string(),
            json!({
                "files": stats.files,
                "reparsed": stats.reparsed,
                "from_cache": stats.from_cache,
            }),
        );
    }
    value
}

/// The failure counterpart of [`envelope`], with the same top-level shape.
///
/// An agent in `--json` mode used to get plain text on stderr for every error,
/// which is unparseable and so breaks silently. This keeps the contract whole.
pub fn error_envelope(
    command: &str,
    kind: &str,
    message: &str,
    suggestion: Option<&str>,
    exit: u8,
    tool_version: &str,
) -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "tool_version": tool_version,
        "command": command,
        "found": false,
        "error": {
            "kind": kind,
            "message": message,
            "suggestion": suggestion,
        },
        "exit": exit,
    })
}
