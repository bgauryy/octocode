//! Bridge locations as the snippets a server answer carries: the location,
//! and the lines it covers read through the request's read policy.
use crate::tools::lsp_search::render::decode_uri_path;
use crate::tools::lsp_search::source::SourceCache;
use octocode_engine::lsp::types::{LspCodeSnippet, LspRange};
use serde_json::{Value, json};

/// Every `Location` and `LocationLink` of `answer`, as snippet rows. An
/// entry with no usable range is skipped, as in a server's answer; a file
/// the policy withholds or that cannot be read keeps its location with the
/// reason as content, so the caller's policy decides what to drop.
pub(super) async fn from_locations(answer: &Value, sources: &mut SourceCache<'_>) -> Vec<Value> {
    let entries: &[Value] = match answer {
        Value::Array(entries) => entries,
        Value::Object(_) => std::slice::from_ref(answer),
        _ => &[],
    };
    let mut found = Vec::new();
    for entry in entries {
        if let Some(snippet) = snippet(entry, sources).await {
            found.push(serde_json::to_value(snippet).unwrap_or(Value::Null));
        }
    }
    found
}

async fn snippet(entry: &Value, sources: &mut SourceCache<'_>) -> Option<LspCodeSnippet> {
    let uri = entry
        .get("uri")
        .or_else(|| entry.get("targetUri"))
        .and_then(Value::as_str)?;
    let context = range_of(entry.get("range").or_else(|| entry.get("targetRange"))?)?;
    // A link separates the symbol's selection from its enclosing declaration.
    let selection = entry.get("targetSelectionRange").and_then(range_of);
    let content = match decode_uri_path(uri) {
        Ok(path) => match sources.get(&path).await {
            Some(source) => lines(&source, &context),
            None => format!("[content unavailable — could not read {uri}]"),
        },
        Err(error) => format!("[content unavailable — {error}]"),
    };
    let display_range = selection.as_ref().map(|_| {
        json!({
            "startLine": context.start.line.saturating_add(1),
            "startCharacter": context.start.character.saturating_add(1),
            "endLine": if context.end.character == 0 {
                context.end.line.max(context.start.line.saturating_add(1))
            } else {
                context.end.line.saturating_add(1)
            }
        })
    });
    Some(LspCodeSnippet {
        uri: uri.to_owned(),
        range: selection.unwrap_or(context),
        content,
        symbol_kind: None,
        display_range,
    })
}

fn range_of(value: &Value) -> Option<LspRange> {
    serde_json::from_value(value.clone()).ok()
}

/// The whole lines `range` covers (LSP ranges end exclusive: one ending at
/// column 0 stops at the line before), without their terminators.
fn lines(source: &crate::tools::lsp_search::source::Source, range: &LspRange) -> String {
    let start = range.start.line as usize;
    let end = if range.end.character == 0 {
        (range.end.line as usize).max(start)
    } else {
        range.end.line as usize + 1
    };
    source
        .lines(start, end)
        .lines()
        .collect::<Vec<_>>()
        .join("\n")
}
