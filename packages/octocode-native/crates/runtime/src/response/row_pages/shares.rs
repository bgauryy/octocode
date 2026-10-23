use super::super::pages::Remaining;
use crate::id::continuation_channels::{HINTS_KEY, PAGES_KEY};
use serde_json::{Map, Value, json};

/// Serialized length `rowPart` adds to a non-empty row object.
pub(super) fn row_part_chars(part: usize, of: usize) -> usize {
    octocode_engine::json_size::utf16_units(&json!({"rowPart": {"part": part, "of": of}}))
        .unwrap_or(0)
        - 1
}

/// The part of a split row a `next.*` call rides. Page continuations
/// (`next*`, `continue*`) resume after the row's last shown evidence, so they
/// ride its last part: following one from an earlier part would silently
/// skip the parts between. Every other call (drill-down, handoff, recovery)
/// rides the first part, beside the head of the evidence. Middle parts carry
/// none, so each call appears exactly once across the part set.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PartShare {
    Whole,
    First,
    Middle,
    Last,
}

impl PartShare {
    pub(super) fn of(part: usize, of: usize) -> Self {
        match (part, of) {
            (_, 1) => Self::Whole,
            (1, _) => Self::First,
            (part, of) if part == of => Self::Last,
            _ => Self::Middle,
        }
    }

    /// Whether this part keeps entry `name` of the row's `container`
    /// (`next` or `hints`). Hint leads and text are never pages, so they
    /// ride the first part.
    fn keeps(self, container: &str, name: &str) -> bool {
        let page = container == PAGES_KEY && crate::id::resumes_after_shown(name);
        match self {
            Self::Whole => true,
            Self::First => !page,
            Self::Middle => false,
            Self::Last => page,
        }
    }
}

/// The `next` and `hints` objects a row carries under `data` and at its top
/// level, with their container name.
fn row_nexts(row: &Value) -> impl Iterator<Item = (&'static str, &Map<String, Value>)> {
    [row.get("data"), Some(row)]
        .into_iter()
        .flatten()
        .flat_map(|slot| {
            [PAGES_KEY, HINTS_KEY].into_iter().filter_map(move |key| {
                slot.get(key)
                    .and_then(Value::as_object)
                    .map(|map| (key, map))
            })
        })
}

/// Serialized `"next":…,`/`"hints":…,` chars a row's `share` keeps.
pub(super) fn row_next_chars(row: &Value, share: PartShare) -> usize {
    row_nexts(row)
        .map(|(container, entries)| {
            let kept: Map<String, Value> = entries
                .iter()
                .filter(|(name, _)| share.keeps(container, name))
                .map(|(name, call)| (name.clone(), call.clone()))
                .collect();
            if kept.is_empty() {
                0
            } else {
                octocode_engine::json_size::utf16_units(&json!({ container: kept })).unwrap_or(0)
                    - 1
            }
        })
        .sum()
}

/// Keep only the continuations and hints a row part rides (see
/// [`PartShare`]); the row's remaining-page `notes` are re-rendered for the
/// pages the part carries or moved.
pub(super) fn keep_row_next(value: &mut Value, share: PartShare, notes: &[Remaining]) {
    let retain = |slot: Option<&mut Map<String, Value>>| {
        let Some(slot) = slot else {
            return;
        };
        let mut moved = Vec::new();
        for container in [PAGES_KEY, HINTS_KEY] {
            if let Some(entries) = slot.get_mut(container).and_then(Value::as_object_mut) {
                entries.retain(|name, _| {
                    let kept = share.keeps(container, name);
                    if !kept && container == PAGES_KEY {
                        moved.push(name.clone());
                    }
                    kept
                });
                if entries.is_empty() {
                    slot.remove(container);
                }
            }
        }
        // Only the first part holds the row's base data; its warning names
        // the pages that moved to the response pager.
        if share == PartShare::First {
            super::super::pages::redisclose(slot, notes, &moved);
        }
    };
    retain(value.get_mut("data").and_then(Value::as_object_mut));
    retain(value.as_object_mut());
}

/// Page-one share of `capacity` for rows of `sizes`: an equal split, where a
/// row smaller than its share keeps only its size and leaves the rest to the
/// larger rows.
pub(crate) fn fair_shares(sizes: &[usize], capacity: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&row| (sizes[row], row));
    let mut shares = vec![0; sizes.len()];
    let mut left = capacity;
    for (taken, &row) in order.iter().enumerate() {
        shares[row] = sizes[row].min(left / (sizes.len() - taken));
        left -= shares[row];
    }
    shares
}
