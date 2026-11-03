use super::evidence::{Catalog, DependencyError, DependencySet};
use super::{
    Open, Slicer, collection_pointer, continues_chars, empty_declared_siblings, enclosing_element,
    lean_declared_skeleton, lean_later_context,
};
use octocode_io::cancellation::Cancellation;
use serde_json::Value;
use std::ops::Range;

mod materialize;
mod records;
mod source_catalog;
mod window;
pub(super) use window::Window;

pub(super) struct Fragment {
    pub(super) kind: FragmentKind,
    pub(super) evidence: Evidence,
}

/// A lazy fragment shape. Source descriptors belong to the complete fragment,
/// so recursive shapes never copy or attach a catalog of their own.
pub(super) enum FragmentKind {
    Whole(usize),
    Records {
        split: usize,
        start: usize,
        end: usize,
    },
    Chunk {
        split: usize,
        start: usize,
        end: usize,
    },
    Joined {
        inner: Box<Self>,
        split: usize,
        before: Range<usize>,
        after: Range<usize>,
    },
    Continues {
        inner: Box<Self>,
        array: String,
        index: usize,
    },
}

#[derive(Clone)]
pub(super) struct Selection {
    catalog: usize,
    dependencies: DependencySet,
}
pub(super) type Evidence = Option<Selection>;
pub(super) type Planned = (Fragment, usize);

impl FragmentKind {
    /// The continuation marker this fragment carries, if any.
    fn marker_mut(&mut self) -> Option<(&mut String, &mut usize)> {
        match self {
            Self::Continues { array, index, .. } => Some((array, index)),
            Self::Joined { inner, .. } => inner.marker_mut(),
            Self::Whole(_) | Self::Records { .. } | Self::Chunk { .. } => None,
        }
    }
}

/// Mark the fragments after `at` as continuing element `index` of `array`.
/// A fragment that already continues an element inside that one (it was
/// planned in a copy holding the element alone, at index 0) keeps its
/// innermost marker, moved into the joined row's coordinates, so the
/// marker's pointer names every continued level.
pub(super) fn mark_continuations(array: &str, at: usize, index: usize, out: &mut [Planned]) {
    for (fragment, size) in out.iter_mut().skip(at + 1) {
        if let Some((nested, nested_index)) = fragment.kind.marker_mut() {
            let before = continues_chars(nested, *nested_index);
            let inside = nested
                .strip_prefix(array)
                .and_then(|rest| rest.strip_prefix("/0"))
                .filter(|rest| rest.is_empty() || rest.starts_with('/'))
                .map(str::to_owned);
            match inside {
                Some(rest) => *nested = format!("{array}/{index}{rest}"),
                // The element itself (index 0 of the copy), or a marker
                // outside it: the element is what continues.
                None => {
                    array.clone_into(nested);
                    *nested_index = index;
                }
            }
            *size = *size - before + continues_chars(nested, *nested_index);
            continue;
        }
        let inner = std::mem::replace(&mut fragment.kind, FragmentKind::Whole(0));
        fragment.kind = FragmentKind::Continues {
            inner: Box::new(inner),
            array: array.to_owned(),
            index,
        };
        *size += continues_chars(array, index);
    }
}

pub(super) struct Skeleton {
    pub(super) value: Value,
    pub(super) chars: usize,
    pub(super) evidence: Evidence,
}

pub(super) struct RowSplit {
    pub(super) contexts: [Skeleton; 2],
    pub(super) pointer: String,
    pub(super) items: Vec<Value>,
    pub(super) evidence: Vec<Evidence>,
}

pub(super) struct RecordSplit {
    pub(super) contexts: [Skeleton; 2],
    pointer: String,
    records: Vec<(String, Value)>,
}

pub(super) struct FragmentArena {
    pub(super) wholes: Vec<Value>,
    pub(super) records: Vec<RecordSplit>,
    /// The tool's declared collections: the only arrays a row splits on.
    pub(super) collection_routes: &'static crate::contracts::output_fields::CollectionRoutes,
    pub(super) splits: Vec<RowSplit>,
    pub(super) page_context: &'static crate::contracts::output_fields::PageContextFields,
    pub(super) catalogs: Vec<Catalog>,
}

impl FragmentArena {
    /// An empty arena that splits rows on `tool`'s declared collections.
    pub(super) fn new(tool: crate::id::ToolId) -> Self {
        let page_context = crate::contracts::output_fields::page_context_fields(tool);
        Self {
            wholes: Vec::new(),
            records: Vec::new(),
            collection_routes: &page_context.collection_routes,
            splits: Vec::new(),
            page_context,
            catalogs: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn plan(
        &mut self,
        mut row: Value,
        head: usize,
        budget: usize,
        out: &mut Vec<Planned>,
    ) -> Result<(), DependencyError> {
        let cancelled = Cancellation::new();
        let catalog = self.take_catalog(&mut row, &cancelled)?;
        self.plan_in(
            row,
            catalog,
            Window {
                head,
                budget,
                terminal_budget: None,
                cancelled: &cancelled,
                continuation_chars: 0,
                tail: 0,
                envelope: 0,
            },
            out,
        )
    }

    pub(super) fn plan_in(
        &mut self,
        mut row: Value,
        catalog: Option<usize>,
        window: Window<'_>,
        out: &mut Vec<Planned>,
    ) -> Result<(), DependencyError> {
        window.check()?;
        let Window {
            head, cancelled, ..
        } = window;
        let evidence = self.dependencies(catalog, &row, cancelled)?;
        let row_chars = size(&row)? + self.evidence_chars(&evidence)?;
        // A row left whole is also its last part: it keeps the tail room.
        let declared = (row_chars + window.tail > head)
            .then(|| collection_pointer(&row, self.collection_routes))
            .flatten();
        if let Some(pointer) = &declared
            && let Some(records) = row.pointer_mut(pointer).and_then(Value::as_object_mut)
        {
            let records = std::mem::take(records);
            return self.plan_records(row, pointer.clone(), records, catalog, window, out);
        }
        let taken = declared.and_then(|pointer| {
            row.pointer_mut(&pointer)
                .and_then(Value::as_array_mut)
                .map(std::mem::take)
                .map(|items| (pointer, items))
        });
        let Some((pointer, items)) = taken else {
            out.push((
                Fragment {
                    kind: FragmentKind::Whole(self.wholes.len()),
                    evidence,
                },
                row_chars,
            ));
            self.wholes.push(row);
            return Ok(());
        };
        let first = out.len();
        if let Some(rest) = self.lean_skeleton(&mut row, window, &pointer, catalog)? {
            self.plan_in(
                rest,
                catalog,
                Window {
                    terminal_budget: None,
                    ..window
                },
                out,
            )?;
            // The rest went first, so even this split's head is a later part.
            lean_later_context(
                &mut row,
                &pointer,
                self.page_context,
                self.collection_routes,
            );
        }
        let contexts = self.contexts(row, &pointer, catalog, cancelled)?;
        let enclosing = enclosing_element(&contexts[0].value, &pointer);
        let marker = enclosing
            .as_ref()
            .map_or(0, |(array, index)| continues_chars(array, *index));
        let window = window.with_context(contexts[0].chars, contexts[1].chars, marker);
        let item_chars = items.iter().map(size).collect::<Result<Vec<_>, _>>()?;
        let item_evidence = items
            .iter()
            .map(|item| self.dependencies(catalog, item, cancelled))
            .collect::<Result<Vec<_>, _>>()?;
        let (open, open_evidence) = if out.len() > first {
            (
                Open::Tail {
                    at: out.len() - 1,
                    start: 0,
                    chars: out.last().map_or(0, |p| p.1),
                },
                out.last().and_then(|p| p.0.evidence.clone()),
            )
        } else {
            (
                Open::Chunk {
                    start: 0,
                    chars: contexts[0].chars,
                },
                contexts[0].evidence.clone(),
            )
        };
        let mut slicer = Slicer {
            split: self.splits.len(),
            enclosing,
            pointer: pointer.clone(),
            first,
            window,
            catalog,
            open,
            open_evidence,
            remaining_chars: item_chars.iter().sum::<usize>() + items.len(),
            terminal_limit: None,
            terminal_checked: false,
        };
        self.splits.push(RowSplit {
            contexts,
            pointer,
            items,
            evidence: item_evidence,
        });
        for (index, chars) in item_chars.iter().enumerate() {
            slicer.push(self, index, chars + 1, out)?;
        }
        slicer.close(self, item_chars.len(), out);
        Ok(())
    }

    fn contexts(
        &self,
        row: Value,
        pointer: &str,
        catalog: Option<usize>,
        cancelled: &Cancellation,
    ) -> Result<[Skeleton; 2], DependencyError> {
        let mut later = row.clone();
        self.empty_secondary_lists(&mut later, pointer);
        lean_later_context(
            &mut later,
            pointer,
            self.page_context,
            self.collection_routes,
        );
        let make = |value: Value| -> Result<Skeleton, DependencyError> {
            let evidence = self.dependencies(catalog, &value, cancelled)?;
            Ok(Skeleton {
                chars: size(&value)? + self.evidence_chars(&evidence)?,
                value,
                evidence,
            })
        };
        Ok([make(row)?, make(later)?])
    }

    pub(super) fn empty_secondary_lists(&self, row: &mut Value, pointer: &str) {
        empty_declared_siblings(row, pointer, self.collection_routes);
    }

    fn lean_skeleton(
        &self,
        row: &mut Value,
        window: Window<'_>,
        pointer: &str,
        catalog: Option<usize>,
    ) -> Result<Option<Value>, DependencyError> {
        lean_declared_skeleton(
            row,
            window.budget,
            pointer,
            self.collection_routes,
            &|value| {
                Ok(size(value)?
                    + self.evidence_chars(&self.dependencies(
                        catalog,
                        value,
                        window.cancelled,
                    )?)?)
            },
        )
    }
}

fn size(value: &Value) -> Result<usize, DependencyError> {
    octocode_engine::json_size::utf16_units(value).ok_or(DependencyError::Serialization)
}
