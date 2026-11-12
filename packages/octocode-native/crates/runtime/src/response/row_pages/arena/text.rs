//! String windows: a row with no declared collection left to split, still
//! larger than its page, is split inside its strings. The largest strings
//! are emptied from the row's skeleton until the skeleton leaves room on a
//! page, then each is cut by [`crate::response::cut::window_end`] into
//! windows sized from the same page budget. Every part after the first names
//! the string it continues (`rowPart.text`, a pointer in the joined row's
//! indexes): appending each part's value there to the previous part's
//! rebuilds the row exactly. The text is sealed already, so a cut never
//! exposes a secret, and the cutter never splits a redaction marker.

use super::{DependencyError, Fragment, FragmentArena, FragmentKind, Planned, Window};
use crate::response::cut::{escaped_len, window_end};
use serde_json::{Value, json};
use std::ops::Range;

/// A row split inside its strings.
pub(in crate::response::row_pages) struct TextSplit {
    /// The row with every windowed string emptied: what the first part shows
    /// beside its window.
    first: Value,
    /// Per windowed string, the lean context a later part shows beside it.
    later: Vec<Value>,
    /// Per windowed string, its pointer in the row this split was planned in.
    pointers: Vec<String>,
    /// Per windowed string, its pointer in its later context, where each
    /// array on the way holds only the element on the way (index 0).
    locals: Vec<String>,
    texts: Vec<String>,
}

/// Serialized length `"text":"<pointer>"` and its comma add to `rowPart`.
pub(in crate::response::row_pages) fn text_marker_chars(pointer: &str) -> usize {
    octocode_engine::json_size::utf16_units(&json!({ "text": pointer })).unwrap_or(0) - 1
}

/// Every string leaf of `value` outside an executable call, with its
/// pointer and escaped length.
fn strings(value: &Value, at: &mut Vec<String>, out: &mut Vec<(String, usize)>) {
    match value {
        Value::String(text) => {
            let mut pointer = String::with_capacity(at.iter().map(|part| part.len() + 1).sum());
            for part in at.iter() {
                pointer.push('/');
                pointer.push_str(part);
            }
            out.push((pointer, escaped_len(text)));
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                at.push(index.to_string());
                strings(item, at, out);
                at.pop();
            }
        }
        // An executable call is indivisible: it runs only whole.
        Value::Object(map) if crate::contracts::output_fields::executable_call(map) => {}
        Value::Object(map) => {
            for (key, item) in map {
                at.push(crate::json_pointer::escape(key));
                strings(item, at, out);
                at.pop();
            }
        }
        _ => {}
    }
}

/// Narrow `value` to the path `pointer` names: each array on the way keeps
/// only the element on the way, and each object only that step, its scalars
/// and the members `context` requires, its lists in the form a later part
/// keeps ([`PageContextFields::keep_later`]: emptied where the schema admits
/// it, so nothing repeats). Returns the pointer in the narrowed value
/// and the innermost element it continues (array pointer in `value`'s
/// indexes, element index), the `rowPart.continues` a later part carries.
///
/// [`PageContextFields::keep_later`]: crate::contracts::output_fields::PageContextFields::keep_later
fn narrow(
    value: &mut Value,
    pointer: &str,
    context: &crate::contracts::output_fields::PageContextFields,
) -> (String, Option<(String, usize)>) {
    let mut node = value;
    let (mut local, mut joined, mut marker) = (String::new(), String::new(), None);
    for segment in pointer.trim_start_matches('/').split('/') {
        let next = match node {
            Value::Array(items) => {
                let Ok(index) = segment.parse::<usize>() else {
                    break;
                };
                if index >= items.len() {
                    break;
                }
                let kept = items.swap_remove(index);
                *items = vec![kept];
                marker = Some((joined.clone(), index));
                local.push_str("/0");
                &mut items[0]
            }
            Value::Object(map) => {
                local.push('/');
                local.push_str(segment);
                let key = crate::json_pointer::unescape(segment);
                map.retain(|name, item| {
                    *name == key
                        || !(item.is_array() || item.is_object())
                        || context
                            .requires(&format!("{joined}/{}", crate::json_pointer::escape(name)))
                });
                // A required list off the path stays, emptied where the
                // schema admits it: its items were delivered by the parts
                // that hold them.
                for (name, item) in map.iter_mut() {
                    if *name != key && item.is_array() {
                        let member = format!("{joined}/{}", crate::json_pointer::escape(name));
                        context.keep_later(&member, item);
                    }
                }
                match map.get_mut(&key) {
                    Some(child) => child,
                    None => break,
                }
            }
            _ => break,
        };
        joined.push('/');
        joined.push_str(segment);
        node = next;
    }
    (local, marker)
}

impl TextSplit {
    /// The context a window of string `string` shows beside it: the row
    /// with every windowed string emptied on the first part, the string's
    /// narrowed context on a later one.
    pub(in crate::response::row_pages) fn context(&self, string: usize, later: bool) -> &Value {
        if later {
            &self.later[string]
        } else {
            &self.first
        }
    }
}

impl FragmentArena {
    /// Plan `row` (`row_chars` long, too large for its window) into string
    /// windows; gives the row back when it holds no string to cut.
    pub(in crate::response::row_pages) fn plan_text(
        &mut self,
        row: Value,
        catalog: Option<usize>,
        window: Window<'_>,
        row_chars: usize,
        out: &mut Vec<Planned>,
    ) -> Result<Option<Value>, DependencyError> {
        self.plan_text_except(row, catalog, window, row_chars, out, &[])
    }

    /// [`Self::plan_text`], keeping the strings at `whole` whole: each fits
    /// one window of the page its part would take.
    fn plan_text_except(
        &mut self,
        mut row: Value,
        catalog: Option<usize>,
        window: Window<'_>,
        row_chars: usize,
        out: &mut Vec<Planned>,
        whole: &[String],
    ) -> Result<Option<Value>, DependencyError> {
        let mut chosen = self.windowed_strings(&row, window.budget, row_chars, whole);
        if chosen.is_empty() {
            return Ok(Some(row));
        }
        // Document order, so a reader appends the strings as the row lists them.
        chosen.sort();
        let texts = chosen
            .iter()
            .map(|pointer| match row.pointer_mut(pointer) {
                Some(Value::String(text)) => std::mem::take(text),
                _ => String::new(),
            })
            .collect::<Vec<_>>();
        // The walks of the elements a windowed string sits in ride its last part.
        let walks = chosen
            .iter()
            .flat_map(|pointer| super::super::take_enclosing_walks(&mut row, pointer))
            .collect::<Vec<_>>();
        let mut later = Vec::with_capacity(chosen.len());
        let mut locals = Vec::with_capacity(chosen.len());
        let mut markers = Vec::with_capacity(chosen.len());
        for pointer in &chosen {
            let mut context = row.clone();
            super::lean_later_context(
                &mut context,
                pointer,
                self.page_context,
                self.collection_routes,
            );
            let (local, marker) = narrow(&mut context, pointer, self.page_context);
            later.push(context);
            locals.push(local);
            markers.push(marker);
        }
        let first_evidence = self.dependencies(catalog, &row, window.cancelled)?;
        let first_chars = super::size(&row)? + self.evidence_chars(&first_evidence)?;
        let split = self.texts.len();
        let mut later_costs = Vec::with_capacity(later.len());
        for ((context, pointer), marker) in later.iter().zip(&chosen).zip(&markers) {
            let evidence = self.dependencies(catalog, context, window.cancelled)?;
            let chars = super::size(context)?
                + self.evidence_chars(&evidence)?
                + text_marker_chars(pointer)
                + marker
                    .as_ref()
                    .map_or(0, |(array, index)| super::continues_chars(array, *index));
            later_costs.push((evidence, chars));
        }
        // Each later window holds at least as much text as the context,
        // markers and tail its part repeats: a context that crowds the page
        // enlarges the window's page (overhead at most half of it, as
        // [`Window::with_context`] sizes a crowded collection's parts)
        // instead of cutting the string into slivers. One rule at any
        // budget, wherever the row sits: a free-text string longer than the
        // page is windowed, never kept whole.
        let crowded = later_costs
            .iter()
            .map(|(_, chars)| window.continuation_chars + chars + window.tail)
            .max()
            .unwrap_or(0);
        let asked = window;
        let window = Window {
            budget: window
                .budget
                .max(crowded.saturating_mul(2).saturating_add(window.envelope)),
            ..window
        };
        let later_room = |chars: usize| {
            window
                .later_limit(window.continuation_chars)
                .saturating_sub(chars + window.tail)
                .max(1)
        };
        // A string that fits one such window stays whole: a part of its own
        // would cost more than the page it saves. A row whose strings all
        // fit is no longer than a balanced page, over the page only by its
        // context; otherwise the rest is planned again with them whole.
        let fitting: Vec<String> = chosen
            .iter()
            .zip(&texts)
            .zip(&later_costs)
            .filter(|((_, text), (_, chars))| escaped_len(text) <= later_room(*chars))
            .map(|((pointer, _), _)| pointer.clone())
            .collect();
        if !fitting.is_empty() {
            for (pointer, text) in chosen.iter().zip(texts) {
                if let Some(slot) = row.pointer_mut(pointer) {
                    *slot = Value::String(text);
                }
            }
            super::super::put_walks(&mut row, &walks);
            if fitting.len() == chosen.len() {
                return Ok(Some(row));
            }
            let whole = whole.iter().cloned().chain(fitting).collect::<Vec<_>>();
            return self.plan_text_except(row, catalog, asked, row_chars, out, &whole);
        }
        let mut first = true;
        for (string, text) in texts.iter().enumerate() {
            let mut start = 0;
            while start < text.len() {
                window.check()?;
                let (room, base, evidence, shown) = if first {
                    (
                        window.head.saturating_sub(first_chars).max(1),
                        first_chars,
                        first_evidence.clone(),
                        None,
                    )
                } else {
                    let (evidence, chars) = &later_costs[string];
                    (
                        later_room(*chars),
                        *chars,
                        evidence.clone(),
                        Some(chosen[string].clone()),
                    )
                };
                let end = window_end(text, start, room);
                let piece = text
                    .get(start..end)
                    .ok_or(DependencyError::Invalid("text window is not a UTF-8 range"))?;
                let later_part = shown.is_some();
                let mut kind = FragmentKind::Text {
                    split,
                    string,
                    range: start..end,
                    shown,
                };
                if later_part && let Some((array, index)) = &markers[string] {
                    kind = FragmentKind::Continues {
                        inner: Box::new(kind),
                        array: array.clone(),
                        index: *index,
                    };
                }
                out.push((
                    Fragment {
                        kind,
                        evidence,
                        walks: Vec::new(),
                    },
                    base + escaped_len(piece),
                ));
                start = end;
                first = false;
            }
        }
        if let Some((fragment, _)) = out.last_mut() {
            fragment.walks.extend(walks);
        }
        self.texts.push(TextSplit {
            first: row,
            later,
            pointers: chosen,
            locals,
            texts,
        });
        Ok(None)
    }

    /// The strings of `row` (`row_chars` long) to window at page room
    /// `room`, largest first, all but those at `whole`.
    fn windowed_strings(
        &self,
        row: &Value,
        room: usize,
        row_chars: usize,
        whole: &[String],
    ) -> Vec<String> {
        let mut found = Vec::new();
        strings(row, &mut Vec::new(), &mut found);
        // Only a string longer than a whole page is windowed, and only free
        // text: a value the output schema bounds (enum, const, format,
        // pattern, length) or does not reach is meaningful only whole, and
        // one every part repeats (an identity, next or disclosure field, or
        // inside one) would arrive on each part again.
        found.retain(|(pointer, len)| {
            *len > room
                && !whole.contains(pointer)
                && crate::contracts::output_fields::free_text_at(self.tool, pointer)
                && !self.page_context.repeats_whole(pointer)
        });
        // Largest first; document order among equals keeps the plan stable.
        found.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        let mut skeleton = row_chars;
        let mut chosen = Vec::new();
        for (pointer, len) in found {
            // A string is windowed while the rest of the row still takes
            // more than half a page: the other half is the windows' room.
            if skeleton.saturating_mul(2) <= room && !chosen.is_empty() {
                break;
            }
            skeleton -= len;
            chosen.push(pointer);
        }
        chosen
    }

    /// The row value of a string window: its context with the window in
    /// place, and, past the first part, the pointer it continues.
    pub(in crate::response::row_pages) fn build_text(
        &self,
        split: usize,
        string: usize,
        range: &Range<usize>,
        shown: Option<&String>,
    ) -> Value {
        let split = &self.texts[split];
        let mut value = split.context(string, shown.is_some()).clone();
        let pointer = match shown {
            None => &split.pointers[string],
            Some(_) => &split.locals[string],
        };
        if let Some(slot) = value.pointer_mut(pointer) {
            // Planning checked these edges against this immutable stored text.
            let tail = split.texts[string].split_at(range.start).1;
            *slot = Value::String(tail.split_at(range.end - range.start).0.to_owned());
        }
        if let Some(shown) = shown {
            value["rowPart"] = json!({ "text": shown });
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn string_paths_keep_unicode_escaped_segments_and_restore_the_parent() {
        let row = json!({
            "é/🦀": {"~field": ["é", {"child": "🦀"}]},
            "z": "tail",
            "next": {"tool": "localFetch", "input": {"path": "indivisible"}},
        });
        let mut path = Vec::new();
        let mut found = Vec::new();
        strings(&row, &mut path, &mut found);
        assert!(path.is_empty());
        assert_eq!(
            found.into_iter().collect::<BTreeMap<_, _>>(),
            BTreeMap::from([
                ("/é~1🦀/~0field/0".to_owned(), 1),
                ("/é~1🦀/~0field/1/child".to_owned(), 2),
                ("/z".to_owned(), 4),
            ])
        );
        let mut found = Vec::new();
        strings(&json!("🦀"), &mut path, &mut found);
        assert_eq!(found, [(String::new(), 2)]);
    }
}
