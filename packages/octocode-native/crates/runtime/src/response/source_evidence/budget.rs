use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProjectionLimits {
    pub bytes: usize,
    pub records: usize,
}

impl Default for ProjectionLimits {
    fn default() -> Self {
        Self {
            bytes: 32 << 20,
            records: 1_048_576,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ProjectionBudget {
    limits: ProjectionLimits,
    bytes: usize,
    records: usize,
}

impl Default for ProjectionBudget {
    fn default() -> Self {
        Self::new(ProjectionLimits::default())
    }
}

impl ProjectionBudget {
    #[cfg(test)]
    pub(super) fn accounting(&self) -> (usize, usize) {
        (self.bytes, self.records)
    }

    pub(crate) fn new(limits: ProjectionLimits) -> Self {
        Self {
            limits,
            bytes: 0,
            records: 0,
        }
    }

    pub(crate) fn admit_json_evidence(
        &mut self,
        value: &serde_json::Value,
        cancel: &dyn CancellationCheck,
    ) -> Result<std::collections::BTreeSet<String>, ToolError> {
        let mut units = std::collections::BTreeSet::new();
        super::visit_origins(value, 0, cancel, &mut |origin, id| {
            self.admit_origin_json(origin, 0, cancel)?;
            self.insert_unit(&mut units, id, cancel)?;
            Ok(())
        })?;
        Ok(units)
    }

    pub(super) fn insert_unit(
        &mut self,
        units: &mut std::collections::BTreeSet<String>,
        id: &str,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), ToolError> {
        cancel.check().map_err(ToolError::cancelled)?;
        if !units.contains(id) {
            self.reserve(96, 1, cancel)?;
            self.string(id, cancel)?;
            units.insert(id.to_owned());
        }
        Ok(())
    }

    fn admit_origin_json(
        &mut self,
        value: &serde_json::Value,
        depth: usize,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), ToolError> {
        cancel.check().map_err(ToolError::cancelled)?;
        if depth > 64 {
            return Err(invalid(
                "source origin exceeds the supported projection nesting depth",
            ));
        }
        match value {
            serde_json::Value::Object(object) => {
                self.record(object.len(), cancel)?;
                for (key, value) in object {
                    self.string(key, cancel)?;
                    self.admit_origin_json(value, depth + 1, cancel)?;
                }
            }
            serde_json::Value::Array(values) => {
                self.reserve(
                    values
                        .capacity()
                        .saturating_mul(std::mem::size_of::<serde_json::Value>()),
                    1,
                    cancel,
                )?;
                for value in values {
                    self.admit_origin_json(value, depth + 1, cancel)?;
                }
            }
            serde_json::Value::String(value) => self.string(value, cancel)?,
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn reserve(
        &mut self,
        bytes: usize,
        records: usize,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), ToolError> {
        cancel.check().map_err(ToolError::cancelled)?;
        let bytes = self.bytes.checked_add(bytes);
        let records = self.records.checked_add(records);
        match (bytes, records) {
            (Some(bytes), Some(records))
                if bytes <= self.limits.bytes && records <= self.limits.records =>
            {
                self.bytes = bytes;
                self.records = records;
                Ok(())
            }
            _ => Err(ToolError::new(
                ErrorCode::ResponseTooLarge,
                format!(
                    "source correspondence and descriptor closure exceed the atomic projection limit ({} bytes, {} records); narrow the requested evidence; no partial origin was returned",
                    self.limits.bytes, self.limits.records,
                ),
            )),
        }
    }

    pub(super) fn string(
        &mut self,
        value: &str,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), ToolError> {
        self.reserve(value.len().saturating_mul(2), 0, cancel)
    }

    pub(super) fn record(
        &mut self,
        fields: usize,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), ToolError> {
        // Reserve both the intermediate JSON map and its generated typed record.
        // This is an allocation-work bound, not a whole-process RSS guarantee.
        let bytes =
            fields.saturating_mul(2 * std::mem::size_of::<(String, serde_json::Value)>() + 64);
        self.reserve(bytes, 1, cancel)
    }
}
