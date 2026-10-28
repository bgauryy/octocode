use super::*;
use octocode_engine::text::analysis as domain;
use serde_json::{Value, json};

fn array<T>(
    values: &[T],
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
    project: impl Fn(&T, &mut ProjectionBudget, &dyn CancellationCheck) -> Result<Value, ToolError>,
) -> Result<Vec<Value>, ToolError> {
    budget.reserve(
        values
            .len()
            .saturating_mul(std::mem::size_of::<Value>() * 2),
        0,
        cancel,
    )?;
    values
        .iter()
        .map(|value| project(value, budget, cancel))
        .collect()
}

fn text(
    value: &str,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    budget.string(value, cancel)?;
    Ok(json!(value))
}

fn range(
    value: &domain::ByteRange,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    budget.record(2, cancel)?;
    if value.start_byte > value.end_byte {
        return Err(invalid("source correspondence has a reversed range"));
    }
    Ok(json!({"startByte": value.start_byte, "endByte": value.end_byte}))
}

fn producer(
    value: &domain::Producer,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    budget.record(2, cancel)?;
    Ok(
        json!({"id": text(&value.id, budget, cancel)?, "version": text(&value.version, budget, cancel)?}),
    )
}

fn original(
    value: &domain::OriginalRange,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    budget.record(2, cancel)?;
    Ok(
        json!({"sourceId": text(&value.source_id, budget, cancel)?, "rawRange": range(&value.raw_range, budget, cancel)?}),
    )
}

fn reason(value: domain::UnmappedReason) -> &'static str {
    match value {
        domain::UnmappedReason::Synthetic => "synthetic",
        domain::UnmappedReason::RawSourceUnavailable => "rawSourceUnavailable",
        domain::UnmappedReason::MappingUnavailable => "mappingUnavailable",
    }
}

fn contribution(
    value: &domain::Contribution,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    budget.record(2, cancel)?;
    Ok(match value {
        domain::Contribution::Source(value) => {
            json!({"kind": "source", "origin": original(value, budget, cancel)?})
        }
        domain::Contribution::Unmapped(value) => {
            json!({"kind": "unmapped", "reason": reason(*value)})
        }
    })
}

fn branch(
    value: &domain::OriginBranch,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    budget.record(2, cancel)?;
    Ok(match value {
        domain::OriginBranch::Copied(value) => {
            json!({"kind": "copied", "origin": original(value, budget, cancel)?})
        }
        domain::OriginBranch::Transcoded(value) => {
            json!({"kind": "transcoded", "origin": original(value, budget, cancel)?})
        }
        domain::OriginBranch::Derived(values) => {
            json!({"kind": "derived", "contributors": array(values, budget, cancel, contribution)?})
        }
        domain::OriginBranch::Unmapped(value) => {
            json!({"kind": "unmapped", "reason": reason(*value)})
        }
    })
}

fn segment(
    value: &domain::OriginSegment,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    budget.record(3, cancel)?;
    let analysis = match value.analysis {
        domain::AnalysisMembership::Included => "included",
        domain::AnalysisMembership::Excluded => "excluded",
    };
    Ok(json!({
        "sourceRange": range(&value.source_range, budget, cancel)?,
        "analysis": analysis,
        "branches": array(&value.branches, budget, cancel, branch)?,
    }))
}

pub(super) fn origin(
    value: &OriginProjection,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<OutputSourceOrigin, ToolError> {
    budget.record(2, cancel)?;
    let value = json!({
        "analysisUnitId": text(&value.analysis_unit_id, budget, cancel)?,
        "segments": array(&value.segments, budget, cancel, segment)?,
    });
    serde_json::from_value(value).map_err(|error| {
        invalid(format!(
            "source origin violates its generated contract: {error}"
        ))
    })
}

fn snapshot(
    value: &domain::SourceSnapshot,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    budget.record(6, cancel)?;
    let encoding = match value.encoding {
        domain::Encoding::Utf8 => "utf8",
        domain::Encoding::Utf16Le => "utf16le",
        domain::Encoding::Utf16Be => "utf16be",
    };
    Ok(json!({
        "id": text(&value.id, budget, cancel)?,
        "path": text(&value.path, budget, cancel)?,
        "digest": text(&value.digest, budget, cancel)?,
        "byteLength": value.byte_length, "encoding": encoding, "bomBytes": value.bom_bytes,
    }))
}

fn representation(
    value: &domain::TextRepresentation,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    budget.record(5, cancel)?;
    Ok(json!({
        "id": text(&value.id, budget, cancel)?,
        "sourceDigest": text(&value.source_digest, budget, cancel)?,
        "byteLength": value.byte_length,
        "producer": producer(&value.producer, budget, cancel)?,
        "sourceIds": array(&value.source_ids, budget, cancel, |id, budget, cancel| text(id, budget, cancel))?,
    }))
}

fn unit(
    value: &domain::AnalysisUnit,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    budget.record(5, cancel)?;
    let operation = match value.operation {
        domain::AnalysisOperation::StructuralSearch => "structuralSearch",
        domain::AnalysisOperation::SyntaxTree => "syntaxTree",
        domain::AnalysisOperation::SignatureOutline => "signatureOutline",
        domain::AnalysisOperation::Declarations => "declarations",
        domain::AnalysisOperation::GraphFacts => "graphFacts",
    };
    Ok(json!({
        "id": text(&value.id, budget, cancel)?,
        "adapter": producer(&value.adapter, budget, cancel)?,
        "operation": operation,
        "representationId": text(&value.representation_id, budget, cancel)?,
        "regions": array(&value.regions, budget, cancel, range)?,
    }))
}

pub(super) fn descriptors(
    value: &SourceDescriptors,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<OutputSourceEvidence, ToolError> {
    budget.record(3, cancel)?;
    let value = json!({
        "snapshots": array(&value.snapshots, budget, cancel, snapshot)?,
        "representations": array(&value.representations, budget, cancel, representation)?,
        "units": array(&value.units, budget, cancel, unit)?,
    });
    serde_json::from_value(value).map_err(|error| {
        invalid(format!(
            "source descriptors violate their generated contract: {error}"
        ))
    })
}
