//! Error types and diagnostics for `MPL` parsing.
use std::fmt::{self};

/// Suggestion for typos / corrections
#[derive(Debug, Clone)]
pub struct Suggestion(String);

impl Suggestion {
    /// The suggested text
    #[must_use]
    pub fn suggestion(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Suggestion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Did you mean \"{}\"?", self.0)
    }
}
