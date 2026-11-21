//! Pure contract checks a request works out again on every repeat: its
//! normalization, its rows' validation, the fields it may not send, and each
//! call its rows offer. Each reads nothing but the embedded contract and its
//! input, so each is kept by its input's text ([`Remembered`]) and read back
//! for the same text.
use octocode_runtime_base::contracts::{self, ContractValidationError};
use octocode_runtime_base::id::ToolId;
use octocode_tools_shared::page_memo::PageMemo;
use serde_json::Value;

/// Results of one pure computation, by the scope it ran under (the tool and
/// mode the input is checked for) and the input's text, in a [`PageMemo`]
/// (the one process budget, dropped when idle).
pub(in crate::runtime::engine) struct Remembered<V> {
    memo: PageMemo<V>,
}

impl<V: Clone + Send + Sync + 'static> Remembered<V> {
    pub(in crate::runtime::engine) const fn new() -> Self {
        Self {
            memo: PageMemo::new(),
        }
    }

    /// What `compute` gives for `input` under `scope`: computed once, then
    /// read back for the same scope and the same input text. `compute` must
    /// depend on nothing else; `weight` prices the result in bytes.
    pub(in crate::runtime::engine) fn get_or(
        &self,
        scope: &str,
        input: &Value,
        compute: impl FnOnce() -> V,
        weight: impl FnOnce(&V) -> usize,
    ) -> V {
        let Ok(text) = serde_json::to_string(input) else {
            return compute();
        };
        let key = format!("{scope}\n{text}");
        if let Some(value) = self.memo.get(&key, "", V::clone) {
            return value;
        }
        let value = compute();
        let bytes = weight(&value);
        self.memo.put(key, String::new(), bytes, value.clone());
        value
    }
}

/// The bytes a kept check result holds: the checked input's JSON, or about
/// a line per issue.
fn checked_weight(checked: &Result<Value, ContractValidationError>) -> usize {
    match checked {
        Ok(value) => octocode_engine::json_size::bytes_or(value, usize::MAX),
        Err(error) => error.issues.len().saturating_mul(256),
    }
}

/// [`octocode_runtime_base::response::continuations::checked_input`], read
/// back for a call checked before: a row offers the same calls on every
/// repeat of a request.
pub(in crate::runtime::engine) fn checked_input(
    target: ToolId,
    related: bool,
    input: &Value,
) -> Result<Value, ContractValidationError> {
    static CHECKED: Remembered<Result<Value, ContractValidationError>> = Remembered::new();
    CHECKED.get_or(
        &format!("{}\n{related}", target.as_str()),
        input,
        || octocode_runtime_base::response::continuations::checked_input(target, related, input),
        checked_weight,
    )
}

/// A request's input as the runtime runs it and the repairs it states
/// ([`contracts::normalized_input`]).
pub(in crate::runtime::engine) fn normalized_input(
    tool: &str,
    input: Value,
) -> (Value, Vec<String>) {
    static NORMALIZED: Remembered<(Value, Vec<String>)> = Remembered::new();
    NORMALIZED.get_or(
        tool,
        &input,
        || contracts::normalized_input(tool, input.clone()),
        |(value, notes)| {
            octocode_engine::json_size::bytes_or(value, usize::MAX)
                .saturating_add(notes.iter().map(String::len).sum())
        },
    )
}

/// What [`contracts::prepare_rows`] (or, for `saved`,
/// [`contracts::prepare_saved_rows`]) gives a request: each row validated,
/// or the envelope's issues.
pub(in crate::runtime::engine) type Prepared =
    Result<Vec<Result<Value, ContractValidationError>>, ContractValidationError>;

/// A request's prepared rows, read back for the same request text.
pub(in crate::runtime::engine) fn prepared_rows(
    tool: &str,
    input: &Value,
    saved: bool,
) -> Prepared {
    static PREPARED: Remembered<Prepared> = Remembered::new();
    PREPARED.get_or(
        &format!("{tool}\n{saved}"),
        input,
        || {
            if saved {
                contracts::prepare_saved_rows(tool, input.clone())
            } else {
                contracts::prepare_rows(tool, input.clone())
            }
        },
        |prepared| match prepared {
            Ok(rows) => rows
                .iter()
                .map(checked_weight)
                .fold(0, usize::saturating_add),
            Err(error) => error.issues.len().saturating_mul(256),
        },
    )
}

#[cfg(test)]
mod tests;
