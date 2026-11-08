//! `lsp.level`: which rung of the resolution ladder answered a row, and the
//! install hint a row without a server carries.
//!
//! The ladder is IDE, server, syntax, text ([`super::ladder::run_page`]). A
//! rung that answers stamps its own level (the IDE and syntax rungs stamp
//! `ide` and `syntax`); every other row is stamped here, once, as it leaves the page:
//! a row whose server was available is a `server` row, and a row no engine
//! answered is a `text` row whose leads are all there is.
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

/// The engine that answered (the contract's `lsp.level`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Level {
    Ide,
    Server,
    Syntax,
    Text,
}

impl Level {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Ide => "ide",
            Self::Server => "server",
            Self::Syntax => "syntax",
            Self::Text => "text",
        }
    }
}

/// Stamp `row`'s `lsp.level` unless a rung already did. A row with no `lsp`
/// receipt (a stale-snapshot or saved-page restart) answered nothing and
/// gets none. A `text` row whose server did not start or was not found
/// names how to install it when the catalog knows and its error does not
/// already say.
pub(super) fn stamp(row: &mut Value) {
    let unavailable = row.get("errorCode") == Some(&json!(ErrorCode::ServerUnavailable));
    let error = row
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let Some(lsp) = row.get_mut("lsp").and_then(Value::as_object_mut) else {
        return;
    };
    if lsp.contains_key("level") {
        return;
    }
    let level = if lsp.get("serverAvailable").and_then(Value::as_bool) == Some(true) {
        Level::Server
    } else {
        Level::Text
    };
    lsp.insert("level".into(), json!(level.name()));
    let hint = (level == Level::Text && unavailable)
        .then(|| install_hint(lsp.get("receipt"), &error))
        .flatten();
    if let Some(hint) = hint {
        crate::response::paging::push_warnings(row, [hint]);
    }
}

/// How to install the server a failed row's receipt names, as a warning;
/// `None` when the catalog knows no hint or `error` already states it.
pub(super) fn install_hint(receipt: Option<&Value>, error: &str) -> Option<String> {
    let command = receipt?.get("command").and_then(Value::as_str)?;
    let name = std::path::Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command);
    let hint = octocode_engine::lsp::config::install_hint(name)?;
    (!error.contains(&hint)).then(|| format!("No {name} server answered: {hint}."))
}

#[cfg(test)]
mod tests;
