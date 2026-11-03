use super::*;
use crate::tools::ast_graph::store::PackedOrigins;
use octocode_engine::text::analysis::{MapBudget, MapLimits, ObservedSpan, SourceDescriptors};

type SpanKey = (u32, u32, u32);

/// One file's source correspondence, held from capture until the snapshot's
/// source table is built. Origins stay encoded against the file's own
/// descriptors: decoded, each range repeats its unit's and source's ids.
#[derive(Clone, Debug, Default)]
pub(crate) struct CapturedSource {
    /// The units, representations and sources the origins cite, each
    /// sorted by id.
    pub descriptors: SourceDescriptors,
    /// The span key of each packed origin, ascending.
    pub keys: Vec<SpanKey>,
    pub origins: PackedOrigins,
    admission: (usize, usize),
}

impl CapturedSource {
    /// The `(bytes, items)` admitting these origins charges the snapshot's
    /// source table ([`read_charge`]): a lower bound of that admission, so
    /// a scan whose running total passes the table's budget can never
    /// publish.
    ///
    /// [`read_charge`]: super::super::limits::read_charge
    pub(crate) fn admission(&self) -> (usize, usize) {
        self.admission
    }
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
        crate::execution::cancel::deadline_of(cancel),
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
    let mut descriptors = view
        .source
        .descriptors(&projections, &mut budget)
        .map_err(ToolError::source)?;
    descriptors.snapshots.sort_by(|a, b| a.id.cmp(&b.id));
    descriptors.representations.sort_by(|a, b| a.id.cmp(&b.id));
    descriptors.units.sort_by(|a, b| a.id.cmp(&b.id));
    let admission = projections
        .iter()
        .fold((0_usize, 0_usize), |(bytes, items), origin| {
            let (more_bytes, more_items) = super::super::limits::read_charge(origin);
            (
                bytes.saturating_add(more_bytes),
                items.saturating_add(more_items),
            )
        });
    let origins =
        PackedOrigins::pack(&projections, &descriptors, cancel).map_err(ToolError::source)?;
    keys.shrink_to_fit();
    Ok(CapturedSource {
        descriptors,
        keys,
        origins,
        admission,
    })
}
