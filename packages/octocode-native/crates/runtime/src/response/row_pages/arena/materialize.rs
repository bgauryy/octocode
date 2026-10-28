#[cfg(test)]
use super::super::drop_emptied_lists;
use super::super::{drop_empty_declared_siblings, note_materialized};
use super::{Fragment, FragmentArena, FragmentKind, RowSplit};
use serde_json::{Value, json};

impl FragmentArena {
    /// Build one planned fragment from a copy of its data: a saved plan
    /// cuts any page again.
    pub(in crate::response::row_pages) fn materialize(&self, fragment: &Fragment) -> Value {
        note_materialized();
        let later = self.later_pointers(&fragment.kind);
        let mut value = self.build(&fragment.kind);
        // Pruned once the fragment is whole: a list the skeleton emptied
        // may be the slot a joined split fills.
        if let Some(context) = self.page_context {
            for (pointer, source) in later {
                if let Some(routes) = self.collection_routes {
                    drop_empty_declared_siblings(
                        &mut value,
                        source,
                        pointer,
                        routes,
                        &context.required,
                    );
                } else {
                    #[cfg(test)]
                    drop_emptied_lists(&mut value, pointer, &context.required);
                }
            }
        }
        value
    }

    /// The paged pointers of `fragment`'s later slices (a chunk past its
    /// split's first item).
    fn later_pointers(&self, fragment: &FragmentKind) -> Vec<(&str, &Value)> {
        match fragment {
            FragmentKind::Whole(_) => Vec::new(),
            FragmentKind::Records { split, start, .. } => (*start > 0)
                .then(|| {
                    (
                        self.records[*split].pointer.as_str(),
                        &self.records[*split].contexts[0].value,
                    )
                })
                .into_iter()
                .collect(),
            FragmentKind::Chunk { split, start, .. } => (*start > 0)
                .then(|| {
                    (
                        self.splits[*split].pointer.as_str(),
                        &self.splits[*split].contexts[0].value,
                    )
                })
                .into_iter()
                .collect(),
            FragmentKind::Joined { inner, .. } | FragmentKind::Continues { inner, .. } => {
                self.later_pointers(inner)
            }
        }
    }

    fn build(&self, fragment: &FragmentKind) -> Value {
        match fragment {
            FragmentKind::Whole(index) => self.wholes[*index].clone(),
            FragmentKind::Records { split, start, end } => {
                let split = &self.records[*split];
                let mut row = split.contexts[usize::from(*start > 0)].value.clone();
                if let Some(slot) = row.pointer_mut(&split.pointer) {
                    *slot = Value::Object(split.records[*start..*end].iter().cloned().collect());
                }
                row
            }
            FragmentKind::Chunk { split, start, end } => {
                let row_split = &self.splits[*split];
                let items = row_split.items[*start..*end].to_vec();
                let mut value = row_split.contexts[usize::from(*start > 0)].value.clone();
                if let Some(slot) = value.pointer_mut(&row_split.pointer) {
                    *slot = Value::Array(items);
                }
                value
            }
            FragmentKind::Joined {
                inner,
                split,
                before,
                after,
            } => {
                let mut value = self.build(inner);
                let RowSplit { pointer, items, .. } = &self.splits[*split];
                if let Some(Value::Array(slot)) = value.pointer_mut(pointer) {
                    let middle = std::mem::take(slot);
                    *slot = items[before.clone()]
                        .iter()
                        .cloned()
                        .chain(middle)
                        .chain(items[after.clone()].iter().cloned())
                        .collect();
                }
                value
            }
            FragmentKind::Continues {
                inner,
                array,
                index,
            } => {
                let mut value = self.build(inner);
                // The outermost element wins over a nested one's marker.
                value["rowPart"] = json!({"continues": {"array": array, "index": index}});
                value
            }
        }
    }
}
