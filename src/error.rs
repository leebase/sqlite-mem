//! The one error type every verb returns.
//!
//! Every failure path in this binary funnels into `AppError` so that
//! `main` has exactly one place that maps an error to (a) an exit code
//! and (b) the `{"ok":false,"error":{code,message,hint}}` envelope on
//! stdout (architecture.md §17). No other module writes to stdout on an
//! error path, and no other module calls `std::process::exit`.

use std::fmt;

/// Exit codes, exactly per architecture.md §17.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    #[allow(dead_code)] // documents the full §17 table; success exits via `main`, not an AppError
    Success = 0,
    Usage = 2,
    Validation = 3,
    NotFound = 4,
    Database = 5,
    VersionMismatch = 6,
    Integrity = 7,
}

impl ExitCode {
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// A machine-readable error, ready to serialize as the `error` object of
/// the stdout envelope.
#[derive(Debug, Clone)]
pub struct AppError {
    pub exit: ExitCode,
    /// Short machine-readable identifier, e.g. `"input_too_large"`.
    pub code: &'static str,
    pub message: String,
    pub hint: Option<String>,
}

impl AppError {
    pub fn new(exit: ExitCode, code: &'static str, message: impl Into<String>) -> Self {
        AppError {
            exit,
            code,
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    // Convenience constructors, one per exit-code family used in S2.

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ExitCode::Usage, "usage_error", message)
    }

    pub fn validation(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(ExitCode::Validation, code, message)
    }

    #[allow(dead_code)] // wired up in a later sprint (forget/ask "not found")
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ExitCode::NotFound, "not_found", message)
    }

    pub fn database(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(ExitCode::Database, code, message)
    }

    pub fn version_mismatch(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(ExitCode::VersionMismatch, code, message)
    }

    // Not used by `info --verify` (S4): that verb's failure envelope must
    // also carry a `checks` object alongside `error` (architecture.md §18,
    // amended), which doesn't fit through the plain AppError -> emit_err
    // path, so it builds its own envelope in `info::run_verify` instead.
    // Kept for any future exit-7 path that doesn't need extra fields.
    #[allow(dead_code)]
    pub fn integrity(message: impl Into<String>) -> Self {
        Self::new(ExitCode::Integrity, "integrity_failed", message)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::database("db_error", e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::database("io_error", e.to_string())
    }
}
