//! A match row cut from a private-key block. A parser reads source unredacted,
//! and one string of a key pasted across lines (`"MIIE…\n" +`) holds no marker
//! the output gate could recognize, so the block is judged over the whole file.
use super::types::Group;
use serde_json::Value;

/// Stands for text a row shows when its source could not be read to check it.
const UNREADABLE: &str = "[REDACTED: source unreadable for secret check]";

/// Hide what `group`'s rows show of `file`'s private-key blocks.
pub(super) fn hide_key_block_rows(
    group: &mut Group,
    file: &std::path::Path,
    paths: &crate::policy::path::PathPolicy,
) {
    let Some(rows) = group.row.get_mut("matches").and_then(Value::as_array_mut) else {
        return;
    };
    let bytes = paths
        .validate_read(file)
        .ok()
        .and_then(|_| octocode_engine::text::open_regular(file).ok())
        .and_then(|(mut opened, _)| {
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut opened, &mut bytes)
                .ok()
                .map(|_| bytes)
        });
    let Some(bytes) = bytes else {
        for row in rows {
            hide_leaves(row, &|_| Some(UNREADABLE));
        }
        return;
    };
    let text = octocode_engine::text::decode(&bytes).text;
    let blocks = crate::security::private_key_block_line_ranges(&text);
    if blocks.is_empty() {
        return;
    }
    let lines = text.lines().collect::<Vec<_>>();
    let placeholder = crate::security::key_fragment_placeholder();
    for row in rows {
        let Some(first) = row["line"].as_u64() else {
            continue;
        };
        let last = row["endLine"].as_u64().unwrap_or(first);
        let hidden = blocks
            .iter()
            .flat_map(|&(start, end)| {
                let from = u64::from(start).max(first);
                let to = u64::from(end).min(last);
                from..=to
            })
            .filter_map(|line| lines.get(usize::try_from(line.checked_sub(1)?).ok()?))
            .collect::<Vec<_>>();
        if !hidden.is_empty() {
            hide_leaves(row, &|line| {
                hidden
                    .iter()
                    .any(|source| source.contains(line))
                    .then_some(placeholder.as_str())
            });
        }
    }
}

/// Replace each line of every string under `value` that `hides` names a
/// replacement for: the row's own text and its capture texts.
fn hide_leaves<'a>(value: &mut Value, hides: &impl Fn(&str) -> Option<&'a str>) {
    match value {
        Value::String(text) => {
            let mut changed = false;
            let lines = text
                .split('\n')
                .map(|line| {
                    let core = crate::security::cut::strip_clip_markers(line).trim();
                    match hides(core).filter(|_| !core.is_empty()) {
                        Some(replacement) => {
                            changed = true;
                            replacement
                        }
                        None => line,
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if changed {
                *text = lines;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| hide_leaves(item, hides)),
        Value::Object(fields) => fields
            .values_mut()
            .for_each(|field| hide_leaves(field, hides)),
        _ => {}
    }
}
