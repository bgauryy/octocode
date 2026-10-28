use super::evidence::{Catalog, DependencyError, DependencySet};
use super::{
    Open, Slicer, collection_pointer, continues_chars, empty_declared_siblings, enclosing_element,
    lean_declared_skeleton, lean_elements,
};
#[cfg(test)]
use super::{empty_secondary_lists, lean_skeleton, take_largest_array};
use serde_json::Value;
use std::ops::Range;
use tokio_util::sync::CancellationToken;

mod materialize;
mod records;

#[derive(Clone, Copy)]
pub(super) struct Window<'a> {
    pub(super) head: usize,
    pub(super) budget: usize,
    pub(super) cancelled: &'a CancellationToken,
    pub(super) continuation_chars: usize,
}

impl Window<'_> {
    pub(super) fn check(self) -> Result<(), DependencyError> {
        if self.cancelled.is_cancelled() {
            Err(DependencyError::Cancelled)
        } else {
            Ok(())
        }
    }
    pub(super) fn with_head(self, head: usize) -> Self {
        Self { head, ..self }
    }

    /// An indivisible context already exceeds the bound. Avoid multiplying
    /// that same oversized context for each tiny child record.
    pub(super) fn with_continuation(self, chars: usize) -> Self {
        Self {
            continuation_chars: self.continuation_chars.max(chars),
            ..self
        }
    }

    pub(super) fn later_limit(self, marker: usize) -> usize {
        self.budget
            .saturating_sub(self.continuation_chars.max(marker))
            .max(1)
    }

    pub(super) fn with_context(self, first: usize, later: usize, marker: usize) -> Self {
        let marker = self.continuation_chars.max(marker);
        Self {
            head: if first > self.budget {
                self.head.max(first.saturating_mul(2))
            } else {
                self.head
            },
            budget: if later > self.later_limit(marker) {
                self.budget
                    .max(later.saturating_mul(2).saturating_add(marker))
            } else {
                self.budget
            },
            ..self
        }
    }
}
mod source_catalog;

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

pub(super) fn mark_continuations(array: &str, at: usize, index: usize, out: &mut [Planned]) {
    for (fragment, size) in out.iter_mut().skip(at + 1) {
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

#[derive(Default)]
pub(super) struct FragmentArena {
    pub(super) wholes: Vec<Value>,
    pub(super) records: Vec<RecordSplit>,
    pub(super) collection_routes:
        Option<&'static crate::contracts::output_fields::CollectionRoutes>,
    pub(super) splits: Vec<RowSplit>,
    pub(super) page_context: Option<&'static crate::contracts::output_fields::PageContextFields>,
    pub(super) catalogs: Vec<Catalog>,
}

impl FragmentArena {
    #[cfg(test)]
    pub(super) fn plan(
        &mut self,
        mut row: Value,
        head: usize,
        budget: usize,
        out: &mut Vec<Planned>,
    ) -> Result<(), DependencyError> {
        let cancelled = CancellationToken::new();
        let catalog = self.take_catalog(&mut row, &cancelled)?;
        self.plan_in(
            row,
            catalog,
            Window {
                head,
                budget,
                cancelled: &cancelled,
                continuation_chars: 0,
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
        let declared = (row_chars > head)
            .then(|| {
                self.collection_routes
                    .and_then(|routes| collection_pointer(&row, routes))
            })
            .flatten();
        if let Some(pointer) = &declared
            && let Some(records) = row.pointer_mut(pointer).and_then(Value::as_object_mut)
        {
            let records = std::mem::take(records);
            return self.plan_records(row, pointer.clone(), records, catalog, window, out);
        }
        let taken = if let Some(pointer) = declared {
            row.pointer_mut(&pointer)
                .and_then(Value::as_array_mut)
                .map(std::mem::take)
                .map(|items| (pointer, items))
        } else {
            #[cfg(test)]
            {
                if row_chars > head && self.collection_routes.is_none() {
                    take_largest_array(&mut row)
                } else {
                    None
                }
            }
            #[cfg(not(test))]
            {
                None
            }
        };
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
            self.plan_in(rest, catalog, window, out)?;
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
        cancelled: &CancellationToken,
    ) -> Result<[Skeleton; 2], DependencyError> {
        let mut later = row.clone();
        self.empty_secondary_lists(&mut later, pointer);
        if let Some(context) = self.page_context {
            lean_elements(&mut later, pointer, context);
        }
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
        if let Some(routes) = self.collection_routes {
            empty_declared_siblings(row, pointer, routes);
        } else {
            #[cfg(test)]
            empty_secondary_lists(row, pointer);
        }
    }

    fn lean_skeleton(
        &self,
        row: &mut Value,
        window: Window<'_>,
        pointer: &str,
        catalog: Option<usize>,
    ) -> Result<Option<Value>, DependencyError> {
        match self.collection_routes {
            Some(routes) => lean_declared_skeleton(row, window.budget, pointer, routes, &|value| {
                Ok(size(value)?
                    + self.evidence_chars(&self.dependencies(
                        catalog,
                        value,
                        window.cancelled,
                    )?)?)
            }),
            None => {
                #[cfg(test)]
                {
                    Ok(lean_skeleton(row, window.budget, pointer))
                }
                #[cfg(not(test))]
                {
                    Ok(None)
                }
            }
        }
    }
}

fn size(value: &Value) -> Result<usize, DependencyError> {
    octocode_engine::json_size::utf16_units(value).ok_or(DependencyError::Serialization)
}
