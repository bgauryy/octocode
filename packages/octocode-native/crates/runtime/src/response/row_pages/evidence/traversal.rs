use super::*;
use std::collections::BTreeSet;

impl Catalog {
    pub(crate) fn dependencies(
        &self,
        value: &Value,
        cancel: &CancellationToken,
    ) -> Result<DependencySet, DependencyError> {
        let mut units = BTreeSet::new();
        let mut pending = Vec::new();
        enqueue(&mut pending, value, cancel)?;
        while let Some(value) = pending.pop() {
            check(cancel)?;
            if let Value::Object(object) = value
                && object.contains_key("analysisUnitId")
            {
                let id = text(value, "analysisUnitId")?;
                let unit = lookup(&self.rows[UNITS], UNITS, id)?;
                let segments = object
                    .get("segments")
                    .and_then(Value::as_array)
                    .filter(|segments| !segments.is_empty())
                    .ok_or(DependencyError::Invalid("source origin has no segments"))?;
                let representation = self.unit_representation[unit];
                for segment in segments {
                    self.validate_sources(segment, representation, cancel)?;
                }
                // The complete origin is validated as one dependency; it is never fragmented.
                units.insert(unit);
            } else {
                enqueue_children(&mut pending, value, cancel)?;
            }
        }
        self.collect_set(units, cancel)
    }

    fn validate_sources(
        &self,
        value: &Value,
        representation: usize,
        cancel: &CancellationToken,
    ) -> Result<(), DependencyError> {
        let mut pending = Vec::new();
        enqueue(&mut pending, value, cancel)?;
        while let Some(value) = pending.pop() {
            check(cancel)?;
            if let Value::Object(object) = value
                && object.contains_key("sourceId")
            {
                let id = text(value, "sourceId")?;
                let index = lookup(&self.rows[SNAPSHOTS], SNAPSHOTS, id)?;
                if self.representation_sources[representation]
                    .binary_search(&index)
                    .is_err()
                {
                    return Err(DependencyError::Invalid(
                        "origin source is outside representation closure",
                    ));
                }
            }
            enqueue_children(&mut pending, value, cancel)?;
        }
        Ok(())
    }
}

fn enqueue<'a>(
    pending: &mut Vec<&'a Value>,
    value: &'a Value,
    cancel: &CancellationToken,
) -> Result<(), DependencyError> {
    check(cancel)?;
    reserve(pending, 1)?;
    pending.push(value);
    Ok(())
}

fn enqueue_children<'a>(
    pending: &mut Vec<&'a Value>,
    value: &'a Value,
    cancel: &CancellationToken,
) -> Result<(), DependencyError> {
    match value {
        Value::Object(object) => {
            for child in object.values().rev() {
                enqueue(pending, child, cancel)?;
            }
        }
        Value::Array(items) => {
            for item in items.iter().rev() {
                enqueue(pending, item, cancel)?;
            }
        }
        _ => {}
    }
    Ok(())
}
