use super::super::result::Remaining;
use crate::response::result::{HINTS_KEY, PAGES_KEY};
use serde_json::{Map, Value, json};

/// Serialized length `rowPart` adds to a non-empty row object.
pub(super) fn row_part_chars(part: usize, of: usize) -> usize {
    octocode_engine::json_size::utf16_units(&json!({"rowPart": {"part": part, "of": of}}))
        .unwrap_or(0)
        - 1
}

/// Required continuations ride the last part so following one cannot skip
/// intermediate evidence. Related calls and guidance ride the first part.
/// Each call appears exactly once across the part set.
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
    fn keeps(self, container: &str) -> bool {
        let page = container == PAGES_KEY;
        match self {
            Self::Whole => true,
            Self::First => !page,
            Self::Middle => false,
            Self::Last => page,
        }
    }
}

fn row_guidance(row: &Value) -> impl Iterator<Item = (&'static str, &Value)> {
    [row.get("data"), Some(row)]
        .into_iter()
        .flatten()
        .flat_map(|slot| {
            [PAGES_KEY, HINTS_KEY, "related"]
                .into_iter()
                .filter_map(move |key| slot.get(key).map(|value| (key, value)))
        })
}

pub(super) fn row_next_chars(row: &Value, share: PartShare) -> usize {
    row_guidance(row)
        .filter(|(key, _)| share.keeps(key))
        .map(|(key, value)| {
            octocode_engine::json_size::utf16_units(&json!({key:value})).unwrap_or(0) - 1
        })
        .sum()
}

pub(super) fn keep_row_next(value: &mut Value, share: PartShare, notes: &[Remaining]) {
    let retain = |slot: Option<&mut Map<String, Value>>| {
        let Some(slot) = slot else {
            return;
        };
        let mut moved = Vec::new();
        for key in [PAGES_KEY, HINTS_KEY, "related"] {
            if !share.keeps(key)
                && let Some(value) = slot.remove(key)
                && key == PAGES_KEY
            {
                moved.extend(
                    value
                        .as_object()
                        .into_iter()
                        .flat_map(|map| map.keys().cloned()),
                );
            }
        }
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
