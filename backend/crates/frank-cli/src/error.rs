//! One error type for command functions that resolve to a single terminal
//! outcome: carries the `frank[ <scope>]:` prefix and exit code, so `main`
//! is the only place that renders an error to stderr instead of every
//! command doing its own `eprintln!(...); return N`.

use std::fmt;

pub struct CliError {
    scope: &'static str,
    message: String,
    pub code: i32,
}

impl CliError {
    pub fn new(scope: &'static str, message: impl fmt::Display, code: i32) -> Self {
        CliError {
            scope,
            message: message.to_string(),
            code,
        }
    }

    /// The common case: `frank: <e>` at exit code 1.
    pub fn general(message: impl fmt::Display) -> Self {
        Self::new("", message, 1)
    }

    /// `frank <scope>: <e>` at exit code 1.
    pub fn scoped(scope: &'static str, message: impl fmt::Display) -> Self {
        Self::new(scope, message, 1)
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scope.is_empty() {
            write!(f, "frank: {}", self.message)
        } else {
            write!(f, "frank {}: {}", self.scope, self.message)
        }
    }
}

/// Render `result` to stderr and return its exit code — the one place a
/// `CliError` becomes process output.
pub fn report(result: Result<(), CliError>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            e.code
        }
    }
}
