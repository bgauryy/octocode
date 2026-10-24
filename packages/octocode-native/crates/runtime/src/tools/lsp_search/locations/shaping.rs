//! Shaping one location: payload sizes, reference totals, context lines,
//! declaration caps and public coordinates.
use super::*;

pub(super) fn location_payload(
    kind: &str,
    provider: &str,
    key: &str,
    rows: Value,
    locations: &[Value],
    pagination: Value,
) -> Value {
    let mut row = json!({"kind":kind});
    row[key] = rows;
    if kind == "references" {
        reference_totals(&mut row, locations);
    }
    row["lsp"] = json!({"serverAvailable":true,"provider":provider});
    row["pagination"] = pagination;
    row
}

/// Characters each location costs a page in the form it prints: a compact
/// `line:col text` row (its file's path once per run of rows), a file
/// summary, or the full row, plus the lines `contextLines` adds around it.
pub(super) fn location_sizes(
    entries: &[Value],
    grouped: bool,
    context_lines: Option<octocode_engine::types::SearchContext>,
) -> Vec<usize> {
    use octocode_engine::json_size::string_body_units;
    entries
        .iter()
        .map(|entry| {
            if grouped {
                return octocode_engine::json_size::utf16_units(entry).unwrap_or(0) + 1;
            }
            let line = entry
                .get("content")
                .and_then(Value::as_str)
                .map_or(0, string_body_units);
            let context = context_lines.map_or(0, |lines| {
                (lines.before.saturating_add(lines.after)) as usize * line
            });
            octocode_engine::json_size::utf16_units(&public_location(entry.clone())).unwrap_or(0)
                + context
                + 1
        })
        .collect()
}

pub(super) fn reference_totals(row: &mut Value, locations: &[Value]) {
    let total_files = locations
        .iter()
        .filter_map(|location| location.get("uri").and_then(Value::as_str))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    // The reference total, whatever the page unit (files when grouped).
    row["matchCount"] = json!(locations.len());
    let recovered = locations
        .iter()
        .filter(|location| location.get("source").and_then(Value::as_str) == Some(RECOVERED_ALIAS))
        .count();
    if recovered > 0 {
        row["recoveredAliasReferences"] = json!(recovered);
    }
    row["matchedFiles"] = json!(total_files);
}

/// Widen a location's `content` to `context_lines` around its range, from
/// the request's source cache (one policy check, read, and line index per
/// file per request).
pub(in super::super) async fn apply_context_lines(
    location: &mut Value,
    context_lines: octocode_engine::types::SearchContext,
    sources: &mut SourceCache<'_>,
) {
    let Some(path) = location.get("uri").and_then(Value::as_str).map(uri_to_path) else {
        return;
    };
    let Some(source) = sources.get(&path).await else {
        return;
    };
    let start_line = location
        .pointer("/range/start/line")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let range_end = location
        .pointer("/range/end/line")
        .and_then(Value::as_u64)
        .unwrap_or(start_line as u64) as usize;
    let end_character = location
        .pointer("/range/end/character")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let selected_end = if end_character == 0 && range_end > start_line {
        range_end - 1
    } else {
        range_end
    };
    if start_line >= source.line_count() {
        return;
    }
    let start = start_line.saturating_sub(context_lines.before as usize);
    let end = selected_end
        .saturating_add(context_lines.after as usize)
        .saturating_add(1)
        .min(source.line_count());
    location["content"] = json!(source.lines(start, end));
    location["displayRange"] = json!({
        "startLine": start + 1,
        "endLine": end,
    });
}

/// Partial reason of a row whose declaration body was clipped at
/// [`MAX_DECLARATION_CONTENT_LINES`].
pub(super) const DECLARATION_CLIPPED: &str = "declarationClipped";

/// Most lines of an enclosing declaration a location carries by default.
/// `contextLines` replaces the body with an explicit window instead.
pub(in super::super) const MAX_DECLARATION_CONTENT_LINES: usize = 60;

/// Keep the first [`MAX_DECLARATION_CONTENT_LINES`] of a location's default
/// declaration body and end it with a marker naming the omitted source lines,
/// so a class definition does not ship its whole body. Returns the localFetch
/// continuation that reads exactly the omitted lines.
pub(in super::super) fn cap_declaration_content(location: &mut Value) -> Option<Value> {
    let content = location.get("content").and_then(Value::as_str)?;
    let lines = content.split('\n').collect::<Vec<_>>();
    if lines.len() <= MAX_DECLARATION_CONTENT_LINES {
        return None;
    }
    let omitted = lines.len() - MAX_DECLARATION_CONTENT_LINES;
    let start = location
        .pointer("/displayRange/startLine")
        .and_then(Value::as_u64)
        .map(|start| start as usize);
    let path = location.get("uri").and_then(Value::as_str).map(uri_to_path);
    let rest = start.map(|start| {
        (
            start + MAX_DECLARATION_CONTENT_LINES,
            start + lines.len() - 1,
        )
    });
    let read = match (&path, rest) {
        (Some(path), Some((from, to))) => Some(
            crate::response::result::Continuation::continued(
                ToolId::LocalFetch,
                json!({"path": path, "ranges": [{"startLine":from,"endLine":to}]}),
            )
            .build(),
        ),
        _ => None,
    };
    let marker = match (rest, read.is_some()) {
        (Some((from, to)), true) => {
            format!("… {omitted} more lines omitted (source lines {from}-{to}).")
        }
        (Some((from, to)), false) => format!(
            "… {omitted} more lines omitted (source lines {from}-{to}); read them with localFetch ranges."
        ),
        (None, _) => format!("… {omitted} more lines omitted; read them with localFetch."),
    };
    let mut capped = lines[..MAX_DECLARATION_CONTENT_LINES].join("\n");
    capped.push('\n');
    capped.push_str(&marker);
    location["content"] = json!(capped);
    read
}

pub(in super::super) fn compact_location(value: Value) -> Value {
    let mut compact = serde_json::Map::new();
    if let Some(uri) = value.get("uri").and_then(Value::as_str) {
        compact.insert("uri".into(), json!(uri));
    }
    if let Some(range) = value.get("range") {
        compact.insert("range".into(), range.clone());
    }
    if let Some(content) = value.get("content") {
        compact.insert("content".into(), content.clone());
    }
    let display_range = value
        .get("displayRange")
        .or_else(|| value.get("display_range"))
        .and_then(normalize_display_range)
        .or_else(|| {
            let start = value.pointer("/range/start/line")?.as_u64()?;
            let end = value.pointer("/range/end/line")?.as_u64()?;
            Some(json!({ "startLine": start + 1, "endLine": end + 1 }))
        });
    if let Some(display_range) = display_range {
        compact.insert("displayRange".into(), display_range);
    }
    let is_definition = value
        .get("isDefinition")
        .or_else(|| value.get("is_definition"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if is_definition {
        compact.insert("isDefinition".into(), json!(true));
    }
    if let Some(source) = value.get("source").and_then(Value::as_str) {
        compact.insert("source".into(), json!(source));
    }
    Value::Object(compact)
}

/// `source` of a reference found through an aliasing import rather than
/// reported by the language server.
pub(in super::super) const RECOVERED_ALIAS: &str = "recoveredAlias";

/// `content` with each source line prefixed by its one-based number and a
/// tab (`<line>\t<text>`, as localFetch reads are numbered), from `start`. A
/// trailing omission marker (see [`cap_declaration_content`]) stays
/// unnumbered.
pub(super) fn number_content(content: &str, start: u64) -> String {
    let mut out = String::with_capacity(content.len() + content.len() / 8);
    let mut line = start;
    for record in content.split_inclusive('\n') {
        if record.starts_with("… ") && record.contains(" more lines omitted") {
            out.push_str(record);
        } else {
            crate::response::numbered::push_numbered(&mut out, line, record);
            line += 1;
        }
    }
    out
}

/// Public location: one one-based `displayRange` (`startLine`,
/// `startCharacter`, `endLine`) for the symbol itself instead of the raw
/// zero-based LSP `range` plus a line-only copy. `content` carries its own
/// line numbers, from the widened window's first line when `contextLines`
/// widened it.
pub(in super::super) fn public_location(internal: Value) -> Value {
    let Value::Object(mut internal) = internal else {
        return internal;
    };
    let range = internal.shift_remove("range");
    let window = internal.shift_remove("displayRange");
    let mut public = serde_json::Map::new();
    if let Some(uri) = internal
        .shift_remove("uri")
        .as_ref()
        .and_then(Value::as_str)
    {
        public.insert("path".into(), json!(uri_to_path(uri)));
    }
    let display = range
        .as_ref()
        .and_then(public_range)
        .or_else(|| window.clone());
    let symbol_start = display
        .as_ref()
        .and_then(|display| display.get("startLine"))
        .and_then(Value::as_u64);
    if let Some(display) = display {
        public.insert("displayRange".into(), display);
    }
    if let Some(content) = internal.shift_remove("content") {
        let start = window
            .as_ref()
            .and_then(|window| window.get("startLine"))
            .and_then(Value::as_u64)
            .or(symbol_start);
        match (content.as_str(), start) {
            // An unreadable file's location carries the reason, not source.
            (Some(text), Some(start))
                if !text.is_empty() && !text.starts_with("[content unavailable") =>
            {
                public.insert("content".into(), json!(number_content(text, start)));
            }
            _ => {
                public.insert("content".into(), content);
            }
        }
    }
    public.append(&mut internal);
    Value::Object(public)
}

pub(super) fn normalize_display_range(value: &Value) -> Option<Value> {
    let start = value
        .get("startLine")
        .or_else(|| value.get("start_line"))
        .and_then(Value::as_u64)?;
    let end = value
        .get("endLine")
        .or_else(|| value.get("end_line"))
        .and_then(Value::as_u64)?;
    Some(json!({ "startLine": start, "endLine": end }))
}
