//! Tests: what building one file costs the build's memory, measured as the
//! build charges it, so budget tests derive their budgets from the file.
use octocode_engine::text::analysis::{AdmittedSource, MapBudget, MapLimits};

/// `count` dense TypeScript functions named `{prefix}N`.
pub(crate) fn dense_source(prefix: &str, count: usize) -> String {
    (0..count)
        .map(|id| {
            format!(
                "export function {prefix}{id}(a, b) {{ return {prefix}{}(b, a); }}\n",
                id + 1
            )
        })
        .collect()
}

/// What one file costs a build.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FilePeak {
    /// What the file holds before its correspondence: the build's first
    /// draw for it (`ScanMemory::base`, its declarations' charge).
    pub entry: usize,
    /// Its bound source plus the least budget its capture fits in.
    pub correspondence: usize,
    /// Its charge once kept ([`crate::tools::ast_graph::limits::build_charge`]
    /// of its facts, declarations and captured bytes).
    pub kept: usize,
}

impl FilePeak {
    /// What the file holds at its peak while it is captured.
    pub(crate) fn in_flight(&self) -> usize {
        self.entry + self.correspondence
    }
}

pub(crate) fn file_peak(text: &str, path: &str) -> FilePeak {
    let facts = octocode_engine::signatures::extract_graph_facts(text, path).unwrap();
    let declarations = facts.declarations.len();
    let unbounded = MapLimits {
        bytes: usize::MAX,
        ..crate::tools::ast_graph::limits::LIMITS.source_map()
    };
    let stop = || false;
    let deadline = crate::execution::cancel::deadline_of(&crate::execution::cancel::NeverCancel);
    let mut budget = MapBudget::new(unbounded, deadline, &stop);
    let admitted = AdmittedSource::from_bytes(text.as_bytes(), path, &mut budget).unwrap();
    let decoded = budget.usage.bytes;
    let bound = facts.source_templates.bind(&admitted, &mut budget).unwrap();
    let bind = budget.usage.bytes - decoded;
    let digest = octocode_engine::digest::sha256(text.as_bytes());
    let view = octocode_engine::fact_scan::GraphFactsView {
        relative_path: path,
        content: text,
        content_digest: &digest,
        source_stamp: None,
        facts: &facts,
        admitted: &admitted,
        source: &bound,
        reference_counts: &[],
        reused: false,
        memory: None,
    };
    let capture = |bytes| {
        super::capture::capture(
            &view,
            &crate::execution::cancel::NeverCancel,
            MapLimits { bytes, ..unbounded },
        )
    };
    let (mut low, mut high) = (0, 1usize << 40);
    while low < high {
        let mid = low + (high - low) / 2;
        if capture(mid).is_ok() {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    let captured = capture(low).expect("fits its own least budget").bytes();
    FilePeak {
        entry: crate::tools::ast_graph::limits::build_charge(0, declarations, 0),
        correspondence: bind + low,
        kept: crate::tools::ast_graph::limits::build_charge(
            octocode_engine::json_size::bytes_or(&facts, usize::MAX),
            declarations,
            captured,
        ),
    }
}
