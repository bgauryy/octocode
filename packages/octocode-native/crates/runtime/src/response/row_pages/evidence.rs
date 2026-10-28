//! Index source descriptors once and select their complete closure for each response fragment.
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

mod sets;
mod traversal;
mod value;
pub(crate) use sets::DependencySet;
use value::{clone_value, size};

const NAMES: [&str; 3] = ["snapshots", "representations", "units"];
const SNAPSHOTS: usize = 0;
const REPRESENTATIONS: usize = 1;
const UNITS: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DependencyError {
    Invalid(&'static str),
    UnknownId {
        collection: &'static str,
        id: String,
    },
    DuplicateId {
        collection: &'static str,
        id: String,
    },
    ForeignSet,
    SizeOverflow,
    Allocation,
    Serialization,
    DepthExceeded,
    Cancelled,
}

impl std::fmt::Display for DependencyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "source-evidence dependency index: {self:?}")
    }
}
impl std::error::Error for DependencyError {}

#[derive(Clone, Copy)]
enum Location {
    Row,
    Data,
}

struct Rows {
    records: Vec<Value>,
    by_id: BTreeMap<String, usize>,
    chars: Vec<usize>,
}

pub(crate) struct Catalog {
    rows: [Rows; 3],
    unit_representation: Vec<usize>,
    representation_sources: Vec<Vec<usize>>,
    owner: Arc<()>,
    location: Location,
    empty_chars: usize,
}

impl Catalog {
    /// Consumes the catalog without cloning its records. On error the caller discards the row.
    pub(crate) fn take(
        row: &mut Value,
        cancel: &CancellationToken,
    ) -> Result<Option<Self>, DependencyError> {
        check(cancel)?;
        let direct = row.get("sourceEvidence").is_some();
        let nested = row
            .get("data")
            .and_then(|data| data.get("sourceEvidence"))
            .is_some();
        let location = match (direct, nested) {
            (false, false) => return Ok(None),
            (true, false) => Location::Row,
            (false, true) => Location::Data,
            (true, true) => return Err(DependencyError::Invalid("two catalog contexts")),
        };
        let parent = parent_mut(row, location)?;
        let value = parent
            .remove("sourceEvidence")
            .ok_or(DependencyError::Invalid("missing catalog"))?;
        let catalog = Self::from_value(value, location, cancel)?;
        // Validate every origin in the original stripped row once, before slicing starts.
        catalog.dependencies(row, cancel)?;
        Ok(Some(catalog))
    }

    fn from_value(
        value: Value,
        location: Location,
        cancel: &CancellationToken,
    ) -> Result<Self, DependencyError> {
        check(cancel)?;
        let Value::Object(mut object) = value else {
            return Err(DependencyError::Invalid("catalog is not an object"));
        };
        let mut take_rows = |name| {
            let Some(Value::Array(records)) = object.remove(name) else {
                return Err(DependencyError::Invalid(
                    "catalog collection is not an array",
                ));
            };
            index_rows(records, name, cancel)
        };
        let rows = [
            take_rows(NAMES[0])?,
            take_rows(NAMES[1])?,
            take_rows(NAMES[2])?,
        ];
        if !object.is_empty() {
            return Err(DependencyError::Invalid("unknown catalog field"));
        }
        if rows[REPRESENTATIONS].records.is_empty() || rows[UNITS].records.is_empty() {
            return Err(DependencyError::Invalid(
                "empty representation or unit collection",
            ));
        }
        let mut representation_sources = Vec::new();
        reserve(
            &mut representation_sources,
            rows[REPRESENTATIONS].records.len(),
        )?;
        for representation in &rows[REPRESENTATIONS].records {
            check(cancel)?;
            let sources = representation
                .get("sourceIds")
                .and_then(Value::as_array)
                .ok_or(DependencyError::Invalid(
                    "representation has no sourceIds array",
                ))?;
            let mut unique = std::collections::BTreeSet::new();
            for source in sources {
                check(cancel)?;
                let id = source
                    .as_str()
                    .ok_or(DependencyError::Invalid("invalid sourceId"))?;
                let index = lookup(&rows[SNAPSHOTS], SNAPSHOTS, id)?;
                if !unique.insert(index) {
                    return Err(DependencyError::Invalid(
                        "duplicate representation sourceId",
                    ));
                }
            }
            let mut indices = Vec::new();
            reserve(&mut indices, unique.len())?;
            for index in unique {
                check(cancel)?;
                indices.push(index);
            }
            representation_sources.push(indices);
        }
        let mut unit_representation = Vec::new();
        reserve(&mut unit_representation, rows[UNITS].records.len())?;
        for unit in &rows[UNITS].records {
            check(cancel)?;
            let id = text(unit, "representationId")?;
            unit_representation.push(lookup(&rows[REPRESENTATIONS], REPRESENTATIONS, id)?);
        }
        let empty_chars = size(
            &serde_json::json!({
                "snapshots": [], "representations": [], "units": []
            }),
            cancel,
        )?;
        Ok(Self {
            rows,
            unit_representation,
            representation_sources,
            owner: Arc::new(()),
            location,
            empty_chars,
        })
    }

    /// A complete catalog in original array order; empty selections have no catalog.
    pub(crate) fn selected(
        &self,
        set: &DependencySet,
        cancel: &CancellationToken,
    ) -> Result<Value, DependencyError> {
        check(cancel)?;
        self.check_owner(set)?;
        if set.is_empty() {
            return Err(DependencyError::Invalid(
                "empty selection must omit sourceEvidence",
            ));
        }
        let mut selected = Map::new();
        for (kind, name) in NAMES.iter().enumerate() {
            let mut records = Vec::new();
            reserve(&mut records, set.indices[kind].len())?;
            for &index in &set.indices[kind] {
                check(cancel)?;
                records.push(clone_value(&self.rows[kind].records[index], cancel, 0)?);
            }
            selected.insert((*name).to_owned(), Value::Array(records));
        }
        Ok(Value::Object(selected))
    }

    /// Adds only selected descriptors. Never changes origin segments, branches, or contributors.
    pub(crate) fn attach(
        &self,
        row: &mut Value,
        set: &DependencySet,
        cancel: &CancellationToken,
    ) -> Result<(), DependencyError> {
        check(cancel)?;
        self.check_owner(set)?;
        let parent = parent_mut(row, self.location)?;
        if parent.contains_key("sourceEvidence") {
            return Err(DependencyError::Invalid(
                "fragment already contains sourceEvidence",
            ));
        }
        if !set.is_empty() {
            parent.insert("sourceEvidence".to_owned(), self.selected(set, cancel)?);
        }
        Ok(())
    }
}

fn parent_mut(
    row: &mut Value,
    location: Location,
) -> Result<&mut Map<String, Value>, DependencyError> {
    let parent = match location {
        Location::Row => row,
        Location::Data => row
            .get_mut("data")
            .ok_or(DependencyError::Invalid("missing data context"))?,
    };
    parent
        .as_object_mut()
        .ok_or(DependencyError::Invalid("catalog context is not an object"))
}

fn index_rows(
    records: Vec<Value>,
    collection: &'static str,
    cancel: &CancellationToken,
) -> Result<Rows, DependencyError> {
    let mut by_id = BTreeMap::new();
    let mut chars = Vec::new();
    reserve(&mut chars, records.len())?;
    for (index, record) in records.iter().enumerate() {
        check(cancel)?;
        let id = text(record, "id")?;
        if by_id.insert(id.to_owned(), index).is_some() {
            return Err(DependencyError::DuplicateId {
                collection,
                id: id.to_owned(),
            });
        }
        chars.push(size(record, cancel)?);
    }
    Ok(Rows {
        records,
        by_id,
        chars,
    })
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, DependencyError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or(DependencyError::Invalid(
            "missing or empty descriptor reference",
        ))
}

fn lookup(rows: &Rows, kind: usize, id: &str) -> Result<usize, DependencyError> {
    rows.by_id
        .get(id)
        .copied()
        .ok_or_else(|| DependencyError::UnknownId {
            collection: NAMES[kind],
            id: id.to_owned(),
        })
}

fn check(cancel: &CancellationToken) -> Result<(), DependencyError> {
    if cancel.is_cancelled() {
        Err(DependencyError::Cancelled)
    } else {
        Ok(())
    }
}

fn reserve<T>(values: &mut Vec<T>, additional: usize) -> Result<(), DependencyError> {
    values
        .try_reserve(additional)
        .map_err(|_| DependencyError::Allocation)
}

#[cfg(test)]
mod tests;
