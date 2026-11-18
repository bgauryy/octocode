use super::*;
use crate::ast_graph::store::PackedOrigins;
use octocode_engine::text::analysis::{
    MapBudget, MapLimits, ObservedSpan, SourceDescriptors, SourceError,
};

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
}

impl CapturedSource {
    /// Bytes it holds: its packed origins, their keys and its descriptors.
    pub(crate) fn bytes(&self) -> usize {
        self.origins.bytes()
            + self.keys.capacity() * std::mem::size_of::<SpanKey>()
            + octocode_engine::json_size::bytes_or(&self.descriptors, 0)
    }
}

/// Whether the file's source correspondence fits `limits` with nothing else
/// held: what [`capture`] would hold, priced one projection at a time
/// ([`octocode_engine::text::analysis::BoundSource::projected_bytes`]), so
/// pricing never holds more than one. Decides a file that cannot be held
/// beside the files kept so far: one that fits alone is the scope's
/// refusal, one that does not is its named skip, the same in any order.
pub(super) fn fits_alone(
    view: &octocode_engine::fact_scan::GraphFactsView<'_>,
    cancel: &dyn CancellationCheck,
    limits: MapLimits,
) -> bool {
    let stop = || cancel.check().is_err();
    let mut budget = MapBudget::new(
        limits,
        octocode_runtime_base::execution::cancel::deadline_of(cancel),
        &stop,
    );
    let mut spans = std::collections::BTreeSet::new();
    view.facts.observed_spans().all(|span| {
        !spans.insert(key(span))
            || (budget.allocate(192).is_ok()
                && view
                    .source
                    .projected_bytes(span, &mut budget)
                    .and_then(|bytes| budget.allocate(bytes))
                    .is_ok())
    })
}

pub(crate) fn key(span: ObservedSpan) -> SpanKey {
    (span.unit.0, span.start_byte, span.end_byte)
}

/// Why a file's source correspondence was not captured.
#[derive(Debug)]
pub(super) enum Uncaptured {
    /// It passed the limit `name` ([`SourceError::LimitExceeded`]);
    /// `exceeded` names that limit, its ceiling and its use
    /// ([`MapBudget::exceeded`]).
    Limit {
        name: &'static str,
        exceeded: String,
    },
    Failed(ToolError),
}

/// The file's source correspondence, held within `limits`
/// ([`super::super::limits::Limits::source_map`]) and drawn, when the scan
/// shares its memory, from the file's hold of it (`view.memory`).
pub(super) fn capture(
    view: &octocode_engine::fact_scan::GraphFactsView<'_>,
    cancel: &dyn CancellationCheck,
    limits: MapLimits,
) -> Result<CapturedSource, Uncaptured> {
    let stop = || cancel.check().is_err();
    let mut budget = MapBudget::new(
        limits,
        octocode_runtime_base::execution::cancel::deadline_of(cancel),
        &stop,
    )
    .drawing(
        view.memory
            .map(|hold| hold as &dyn octocode_engine::text::analysis::Draw),
    );
    let failed = |error: SourceError, budget: &MapBudget<'_>| match error {
        SourceError::LimitExceeded(name) => Uncaptured::Limit {
            name,
            exceeded: budget.exceeded(name),
        },
        error => Uncaptured::Failed(ToolError::source(error)),
    };
    let mut keys = Vec::new();
    let mut projections = Vec::new();
    let mut spans = BTreeMap::new();
    for span in view.facts.observed_spans() {
        budget.check().map_err(|error| failed(error, &budget))?;
        if let std::collections::btree_map::Entry::Vacant(entry) = spans.entry(key(span)) {
            budget
                .allocate(192)
                .map_err(|error| failed(error, &budget))?;
            entry.insert(span);
        }
    }
    for (key, span) in spans {
        keys.push(key);
        projections.push(
            view.source
                .project(span, &mut budget)
                .map_err(|error| failed(error, &budget))?,
        );
    }
    let mut descriptors = view
        .source
        .descriptors(&projections, &mut budget)
        .map_err(|error| failed(error, &budget))?;
    descriptors.snapshots.sort_by(|a, b| a.id.cmp(&b.id));
    descriptors.representations.sort_by(|a, b| a.id.cmp(&b.id));
    descriptors.units.sort_by(|a, b| a.id.cmp(&b.id));
    let origins = PackedOrigins::pack(&projections, &descriptors, cancel)
        .map_err(|error| Uncaptured::Failed(ToolError::source(error)))?;
    keys.shrink_to_fit();
    Ok(CapturedSource {
        descriptors,
        keys,
        origins,
    })
}
