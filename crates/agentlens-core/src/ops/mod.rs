pub mod find;
pub mod literals;
pub mod map;
pub mod slice;

use serde_json::Value;

#[derive(Debug)]
pub struct Report {
    pub text: String,
    pub json: Value,
    pub found: bool,
}

impl Report {
    pub fn new(text: String, json: Value, found: bool) -> Self {
        Self { text, json, found }
    }
}
