use super::*;
use octocode_engine::text::analysis::{
    MapBudget, MapLimits, ObservedSpan, OriginProjection, SourceDescriptors,
};
use std::time::{Duration, Instant};

type SpanKey = (u32, u32, u32);

#[derive(Clone, Debug, Default)]
pub(crate) struct CapturedSource {
    pub descriptors: SourceDescriptors,
    pub origins: BTreeMap<SpanKey, OriginProjection>,
}

pub(crate) fn key(span: ObservedSpan) -> SpanKey {
    (span.unit.0, span.start_byte, span.end_byte)
}

pub(super) fn capture(
    view: &octocode_engine::fact_scan::GraphFactsView<'_>,
    cancel: &dyn CancellationCheck,
) -> Result<CapturedSource, ToolError> {
    let stop = || cancel.check().is_err();
    let mut budget = MapBudget::new(
        MapLimits::default(),
        Instant::now() + Duration::from_secs(30),
        &stop,
    );
    let mut keys = Vec::new();
    let mut projections = Vec::new();
    let mut spans = BTreeMap::new();
    for span in view.facts.observed_spans() {
        budget.check().map_err(ToolError::source)?;
        if let std::collections::btree_map::Entry::Vacant(entry) = spans.entry(key(span)) {
            budget.allocate(192).map_err(ToolError::source)?;
            entry.insert(span);
        }
    }
    for (key, span) in spans {
        keys.push(key);
        projections.push(
            view.source
                .project(span, &mut budget)
                .map_err(ToolError::source)?,
        );
    }
    let descriptors = view
        .source
        .descriptors(&projections, &mut budget)
        .map_err(ToolError::source)?;
    Ok(CapturedSource {
        descriptors,
        origins: keys.into_iter().zip(projections).collect(),
    })
}
