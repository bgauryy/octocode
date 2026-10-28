//! The shared boundary from engine correspondence to generated public evidence.
use crate::execution::cancel::CancellationCheck;
use crate::response::result::ToolError;
use octocode_contract_types::{
    ErrorCode, OutputSourceByteRange, OutputSourceEvidence, OutputSourceOrigin,
};
use octocode_engine::text::analysis::{OriginProjection, SourceDescriptors};

mod budget;
mod catalog;
mod wire;
pub(crate) use budget::{ProjectionBudget, ProjectionLimits};
#[cfg(test)]
use catalog::validate_origin;
pub(crate) use catalog::{merge_descriptors, validate_catalog};

pub(crate) fn project_range(
    range: octocode_engine::text::analysis::ByteRange,
) -> Result<OutputSourceByteRange, ToolError> {
    if range.start_byte > range.end_byte {
        return Err(invalid("source correspondence has a reversed range"));
    }
    serde_json::from_value(
        serde_json::json!({"startByte": range.start_byte, "endByte": range.end_byte}),
    )
    .map_err(|error| {
        invalid(format!(
            "source range violates its generated contract: {error}"
        ))
    })
}

pub(crate) fn source_digest<'a>(
    descriptors: &'a SourceDescriptors,
    unit_id: &str,
) -> Result<&'a str, ToolError> {
    let unit = descriptors
        .units
        .iter()
        .find(|unit| unit.id == unit_id)
        .ok_or_else(|| invalid("source origin has no analysis unit descriptor"))?;
    descriptors
        .representations
        .iter()
        .find(|representation| representation.id == unit.representation_id)
        .map(|representation| representation.source_digest.as_str())
        .ok_or_else(|| invalid("source origin has no representation descriptor"))
}

pub(crate) fn project_origin(
    origin: &OriginProjection,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<OutputSourceOrigin, ToolError> {
    wire::origin(origin, budget, cancel)
}

pub(crate) fn project_descriptors<'a>(
    descriptors: &SourceDescriptors,
    unit_ids: impl IntoIterator<Item = &'a str>,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<OutputSourceEvidence, ToolError> {
    let selected = catalog::closure(descriptors, unit_ids, budget, cancel)?;
    wire::descriptors(&selected, budget, cancel)
}

fn invalid(message: impl Into<String>) -> ToolError {
    ToolError::new(ErrorCode::ExecutionFailed, message)
}

pub(crate) fn referenced_units(
    value: &serde_json::Value,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<std::collections::BTreeSet<String>, ToolError> {
    let mut ids = std::collections::BTreeSet::new();
    visit_origins(value, 0, cancel, &mut |_, id| {
        budget.insert_unit(&mut ids, id, cancel)?;
        Ok(())
    })?;
    Ok(ids)
}

fn visit_origins(
    value: &serde_json::Value,
    depth: usize,
    cancel: &dyn CancellationCheck,
    visit: &mut impl FnMut(&serde_json::Value, &str) -> Result<(), ToolError>,
) -> Result<(), ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    if depth > 64 {
        return Err(invalid(
            "source evidence nesting exceeds the projection traversal limit",
        ));
    }
    match value {
        serde_json::Value::Object(object) => {
            if let Some(id) = object
                .get("analysisUnitId")
                .and_then(serde_json::Value::as_str)
            {
                if !object
                    .get("segments")
                    .is_some_and(serde_json::Value::is_array)
                {
                    return Err(invalid("emitted source origin has no segments"));
                }
                visit(value, id)?;
            } else {
                for value in object.values() {
                    visit_origins(value, depth + 1, cancel, visit)?;
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                visit_origins(value, depth + 1, cancel, visit)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests;
