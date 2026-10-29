use super::{
    DependencyError, Fragment, FragmentArena, FragmentKind, Planned, RecordSplit, Window,
    continues_chars, enclosing_element, mark_continuations, size,
};
use serde_json::{Map, Value};

impl FragmentArena {
    pub(super) fn plan_records(
        &mut self,
        mut row: Value,
        pointer: String,
        records: Map<String, Value>,
        catalog: Option<usize>,
        window: Window<'_>,
        out: &mut Vec<Planned>,
    ) -> Result<(), DependencyError> {
        window.check()?;
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
        }
        let contexts = self.contexts(row, &pointer, catalog, window.cancelled)?;
        let enclosing = enclosing_element(&contexts[0].value, &pointer);
        let marker = enclosing
            .as_ref()
            .map_or(0, |(array, index)| continues_chars(array, *index));
        let window = window.with_context(contexts[0].chars, contexts[1].chars, marker);
        let split = self.records.len();
        let records: Vec<_> = records.into_iter().collect();
        let sizes = records
            .iter()
            .map(|(key, value)| Ok(size(&Value::String(key.clone()))? + 1 + size(value)?))
            .collect::<Result<Vec<_>, DependencyError>>()?;
        let dependencies = records
            .iter()
            .map(|(_, value)| self.dependencies(catalog, value, window.cancelled))
            .collect::<Result<Vec<_>, _>>()?;
        let mut start = 0;
        let mut chars = contexts[0].chars;
        let mut evidence = contexts[0].evidence.clone();
        let mut limit = if out.len() == first {
            window.head
        } else {
            window.later_limit(marker)
        };
        self.records.push(RecordSplit {
            contexts,
            pointer,
            records,
        });
        for (index, item) in sizes.iter().copied().enumerate() {
            window.check()?;
            let extra = usize::from(index > start)
                + item
                + self.evidence_delta(&evidence, &dependencies[index], window.cancelled)?;
            let tail = window.tail_of(index, sizes.len());
            if index > start && chars + extra + tail > limit {
                out.push((
                    Fragment {
                        kind: FragmentKind::Records {
                            split,
                            start,
                            end: index,
                        },
                        evidence,
                    },
                    chars,
                ));
                start = index;
                let base = &self.records[split].contexts[1];
                chars = base.chars;
                evidence = base.evidence.clone();
                limit = window.later_limit(marker);
            }
            let extra = usize::from(index > start)
                + item
                + self.evidence_delta(&evidence, &dependencies[index], window.cancelled)?;
            if index == start && chars + extra + tail > limit {
                let record = &mut self.records[split];
                let mut single = record.contexts[usize::from(index > 0)].value.clone();
                let (key, value) = &mut record.records[index];
                if let Some(slot) = single.pointer_mut(&record.pointer) {
                    *slot = Value::Object(Map::from_iter([(key.clone(), std::mem::take(value))]));
                }
                self.plan_in(
                    single,
                    catalog,
                    window.for_item(index, sizes.len()).with_head(limit),
                    out,
                )?;
                start = index + 1;
                let base = &self.records[split].contexts[1];
                chars = base.chars;
                evidence = base.evidence.clone();
                limit = window.later_limit(marker);
                continue;
            }
            chars += extra;
            self.merge_evidence(&mut evidence, &dependencies[index], window.cancelled)?;
        }
        if start < sizes.len() {
            out.push((
                Fragment {
                    kind: FragmentKind::Records {
                        split,
                        start,
                        end: sizes.len(),
                    },
                    evidence,
                },
                chars,
            ));
        }
        if let Some((array, index)) = enclosing {
            mark_continuations(&array, first, index, out);
        }
        Ok(())
    }
}
