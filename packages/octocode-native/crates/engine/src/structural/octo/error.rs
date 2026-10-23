//! Why a structural query does not compile. The diagnostic code and its
//! message are derived from the variant, so no caller parses a code back out
//! of text; `Display` (`[code] message`) is the one-line form for a bare
//! `String` error.

use crate::structural::types::StructuralDiagnostic;

use super::matching::{ExecutionError, PARSE_INTERRUPTED};
use super::rule::RULE_INVALID;

/// The code of a rule or pattern the grammar cannot compile.
pub const COMPILE_FAILED: &str = "structural.query.compileFailed";

#[derive(Debug, Clone)]
pub(in crate::structural) enum CompileError {
    /// A rule that does not parse as a rule: a wrong value type, an unknown key,
    /// no matcher, a bad regex.
    Rule(String),
    /// Compiling a pattern needed a parse that failed or was interrupted.
    Parse(ExecutionError),
    /// A rule or pattern this grammar cannot compile.
    Grammar(String),
}

impl CompileError {
    pub(super) fn rule(message: impl Into<String>) -> Self {
        Self::Rule(message.into())
    }

    pub(super) fn grammar(message: impl Into<String>) -> Self {
        Self::Grammar(message.into())
    }

    /// The compile-time parse interruption, which a caller reports as a
    /// truncated file rather than a failed query.
    pub(in crate::structural) fn interruption(&self) -> Option<&ExecutionError> {
        match self {
            Self::Parse(error) if error.code == PARSE_INTERRUPTED => Some(error),
            _ => None,
        }
    }

    /// The text of the error, without its code.
    fn message(&self) -> &str {
        match self {
            Self::Rule(message) | Self::Grammar(message) => message,
            Self::Parse(error) => &error.message,
        }
    }

    /// The diagnostic of a query that does not compile: its code, and the
    /// message without it.
    pub(in crate::structural) fn diagnostic(&self) -> StructuralDiagnostic {
        match self {
            Self::Rule(_) => {
                StructuralDiagnostic::new(RULE_INVALID, "error", "match", self.message())
                    .with_recovery(
                        "Fix the rule object: the message names the key or value it rejects.",
                    )
            }
            Self::Parse(_) | Self::Grammar(_) => StructuralDiagnostic::new(
                COMPILE_FAILED,
                "error",
                "match",
                self.message(),
            )
            .with_recovery(
                "Check the structural pattern or rule object against this file's language grammar.",
            ),
        }
    }
}

impl From<ExecutionError> for CompileError {
    fn from(error: ExecutionError) -> Self {
        Self::Parse(error)
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rule(message) => write!(f, "[{RULE_INVALID}] {message}"),
            Self::Parse(error) => std::fmt::Display::fmt(error, f),
            Self::Grammar(message) => write!(f, "[{COMPILE_FAILED}] {message}"),
        }
    }
}
