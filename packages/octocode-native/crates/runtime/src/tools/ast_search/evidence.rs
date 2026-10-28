use crate::execution::cancel::CancellationCheck;
use crate::response::result::ToolError;
use crate::response::source_evidence::{self, ProjectionBudget, ProjectionLimits};
use octocode_engine::text::analysis::{
    BoundSource, MapBudget, ObservedSpan, SourceDescriptors, UnitHandle,
};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) struct FileEvidence<'source, 'budget, 'cancel> {
    source: &'source BoundSource<'source>,
    maps: &'budget mut MapBudget<'cancel>,
    projection: ProjectionBudget,
    cancel: &'cancel dyn CancellationCheck,
    units: BTreeSet<UnitHandle>,
}

impl<'source, 'budget, 'cancel> FileEvidence<'source, 'budget, 'cancel> {
    pub(super) fn new(
        source: &'source BoundSource<'source>,
        maps: &'budget mut MapBudget<'cancel>,
        cancel: &'cancel dyn CancellationCheck,
    ) -> Self {
        Self {
            source,
            maps,
            projection: ProjectionBudget::new(ProjectionLimits::default()),
            cancel,
            units: BTreeSet::new(),
        }
    }

    pub(super) fn span(
        &mut self,
        row: &mut Value,
        span: ObservedSpan,
        range_key: &str,
        origin_key: &str,
        digest: bool,
    ) -> Result<(), ToolError> {
        let origin = self
            .source
            .project(span, self.maps)
            .map_err(ToolError::source)?;
        let origin = source_evidence::project_origin(&origin, &mut self.projection, self.cancel)?;
        let range = source_evidence::project_range(span.range)?;
        row[range_key] = serde_json::to_value(range).map_err(encoding_error)?;
        row[origin_key] = serde_json::to_value(origin).map_err(encoding_error)?;
        if digest {
            row["sourceDigest"] = Value::String(
                self.source
                    .representation_for_unit(span.unit)
                    .map_err(ToolError::source)?
                    .source_digest
                    .clone(),
            );
        }
        self.units.insert(span.unit);
        Ok(())
    }

    pub(super) fn finish(self) -> Result<SourceDescriptors, ToolError> {
        self.source
            .descriptors_for_units(&self.units.into_iter().collect::<Vec<_>>(), self.maps)
            .map_err(ToolError::source)
    }
}

fn encoding_error(error: serde_json::Error) -> ToolError {
    ToolError::new(
        octocode_contract_types::ErrorCode::ExecutionFailed,
        format!("Could not encode source correspondence: {error}"),
    )
}

pub(super) fn attach_page(
    row: &mut Value,
    descriptors: &SourceDescriptors,
    cancel: &dyn CancellationCheck,
) -> Result<(), ToolError> {
    let mut budget = ProjectionBudget::new(ProjectionLimits::default());
    let ids = budget.admit_json_evidence(row, cancel)?;
    if ids.is_empty() {
        return Ok(());
    }
    let projected = source_evidence::project_descriptors(
        descriptors,
        ids.iter().map(String::as_str),
        &mut budget,
        cancel,
    )?;
    row["sourceEvidence"] = serde_json::to_value(projected).map_err(encoding_error)?;
    Ok(())
}
