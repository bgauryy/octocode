//! Every graph build and snapshot bound, owned once. A build is refused as
//! soon as its snapshot can no longer publish; writers take [`Limits`]
//! (tests lower them), readers decode untrusted snapshots under [`LIMITS`].
use std::time::Duration;

/// The bounds one graph build and its snapshot obey.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Limits {
    /// `graph.bin` bytes, written and read.
    pub snapshot_bytes: usize,
    /// Rows per snapshot table.
    pub table_items: usize,
    /// EDGE rows. The build tallies its certain edges against the same
    /// bound, so a build that passes it can still publish.
    pub edges: usize,
    /// Expanded bytes of both string dictionaries (STRS and STRE).
    pub string_bytes: usize,
    /// Allocation charge of the source table (SRCE).
    pub source_bytes: usize,
    /// Mapping work of the source table.
    pub source_work: usize,
    /// Wall clock of one source-table pass. The write side keeps it too:
    /// CLI `graph ingest` runs under no outer cancel or deadline.
    pub source_deadline: Duration,
    /// Checkpoint artifacts on disk.
    pub spill_bytes: u64,
    /// Artifacts held in memory because they hold secrets.
    pub memory_only_bytes: u64,
    /// One extraction artifact.
    pub artifact_bytes: usize,
    /// Files walked when the caller names no bound.
    pub max_files: u32,
    /// Source correspondence of one file at capture.
    pub file_map_deadline: Duration,
}

pub(crate) const LIMITS: Limits = Limits {
    snapshot_bytes: 512 << 20,
    table_items: 5_000_000,
    edges: 2_000_000,
    string_bytes: 512 << 20,
    source_bytes: 512 << 20,
    source_work: 512 << 20,
    source_deadline: Duration::from_secs(30),
    spill_bytes: 2 << 30,
    memory_only_bytes: 64 << 20,
    artifact_bytes: 64 << 20,
    max_files: 50_000,
    file_map_deadline: Duration::from_secs(30),
};

/// Sections one snapshot may hold (format shape, read side).
pub(crate) const MAX_SECTIONS: usize = 64;

// The bounds agree, or the crate does not build: every EDGE row is a table
// row, and no part of a snapshot may outgrow the whole.
const _: () = {
    assert!(LIMITS.edges <= LIMITS.table_items);
    assert!(LIMITS.source_bytes <= LIMITS.snapshot_bytes);
    assert!(LIMITS.string_bytes <= LIMITS.snapshot_bytes);
    assert!(LIMITS.artifact_bytes as u64 <= LIMITS.spill_bytes);
    assert!(LIMITS.memory_only_bytes <= LIMITS.spill_bytes);
};

impl Default for Limits {
    fn default() -> Self {
        LIMITS
    }
}

/// Snapshot rows a build is certain to project: a lower bound of the NODE
/// and EDGE tables, so a tally past a bound is a snapshot that cannot
/// publish, never a false refusal.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Tally {
    pub nodes: usize,
    pub edges: usize,
}

impl Tally {
    /// One linked file: its file node, a symbol node per declaration and
    /// the `contains` edge that owns each symbol.
    pub(crate) fn file(declarations: usize) -> Self {
        Self {
            nodes: declarations.saturating_add(1),
            edges: declarations,
        }
    }

    pub(crate) fn add(&mut self, other: Self) {
        self.nodes = self.nodes.saturating_add(other.nodes);
        self.edges = self.edges.saturating_add(other.edges);
    }
}

impl Limits {
    /// The EDGE rows a reader admits: the edge bound, within the table bound.
    pub(crate) fn edge_rows(&self) -> usize {
        self.edges.min(self.table_items)
    }

    /// The table (`NODE` or `EDGE`) `tally` already overfills, with its
    /// bound. A tally exactly at a bound publishes.
    pub(crate) fn admit(&self, tally: &Tally) -> Result<(), (&'static str, usize)> {
        if tally.nodes > self.table_items {
            return Err(("NODE", self.table_items));
        }
        if tally.edges > self.edge_rows() {
            return Err(("EDGE", self.edge_rows()));
        }
        Ok(())
    }
}

/// Bytes of one origin's entry in the source table's index (SRCE); the
/// table asserts its entry has this size.
pub(crate) const ORIGIN_INDEX_BYTES: usize = 24;

/// What admitting `origin` back from a published source table charges:
/// its index entry plus its decode (unit id, every vector, every raw
/// source id), every item and every byte but its encoded record. A scan's
/// running sum of it bounds the table from below before it is built.
pub(crate) fn read_charge(
    origin: &octocode_engine::text::analysis::OriginProjection,
) -> (usize, usize) {
    use octocode_engine::text::analysis::{
        Contribution, OriginBranch, OriginSegment, OriginalRange,
    };
    use std::mem::size_of;
    let mut bytes = ORIGIN_INDEX_BYTES
        + origin.analysis_unit_id.len()
        + origin.segments.len() * size_of::<OriginSegment>();
    let mut items = 2 + origin.segments.len();
    let raw = |value: &OriginalRange, bytes: &mut usize, items: &mut usize| {
        *bytes += value.source_id.len();
        *items += 1;
    };
    for segment in &origin.segments {
        bytes += segment.branches.len() * size_of::<OriginBranch>();
        items += segment.branches.len();
        for branch in &segment.branches {
            match branch {
                OriginBranch::Copied(value) | OriginBranch::Transcoded(value) => {
                    raw(value, &mut bytes, &mut items);
                }
                OriginBranch::Derived(values) => {
                    bytes += values.len() * size_of::<Contribution>();
                    items += values.len();
                    for value in values {
                        if let Contribution::Source(value) = value {
                            raw(value, &mut bytes, &mut items);
                        }
                    }
                }
                OriginBranch::Unmapped(_) => {}
            }
        }
    }
    (bytes, items)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The build's bounds can never pass what the reader admits (the old
    /// 2M file-edge cap counted a subset of the 2M EDGE rows, so a build at
    /// it could not publish). The orderings are checked at compile time.
    #[test]
    fn limits_are_consistent() {
        assert_eq!(LIMITS.edge_rows(), LIMITS.edges);
        assert_eq!(
            Limits::default(),
            LIMITS,
            "writers default to the reader's bounds"
        );
    }

    #[test]
    fn a_tally_at_a_bound_is_admitted_and_one_past_it_names_the_table() {
        let limits = Limits {
            table_items: 4,
            edges: 3,
            ..LIMITS
        };
        assert_eq!(limits.admit(&Tally { nodes: 4, edges: 3 }), Ok(()));
        assert_eq!(
            limits.admit(&Tally { nodes: 5, edges: 0 }),
            Err(("NODE", 4))
        );
        assert_eq!(
            limits.admit(&Tally { nodes: 1, edges: 4 }),
            Err(("EDGE", 3))
        );
        let narrow = Limits {
            table_items: 2,
            ..limits
        };
        assert_eq!(
            narrow.admit(&Tally { nodes: 1, edges: 3 }),
            Err(("EDGE", 2)),
            "EDGE rows are table rows too"
        );
        let mut tally = Tally::default();
        tally.add(Tally::file(2));
        tally.add(Tally::file(0));
        assert_eq!(tally, Tally { nodes: 4, edges: 2 });
    }

    /// Tests lower a bound one way, through [`Limits`]: no thread-local
    /// cap, per-budget constructor or second edge cap is left beside it.
    #[test]
    fn bounds_are_lowered_only_through_limits() {
        fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("source dir").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    out.push(path);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tools/ast_graph");
        let mut files = Vec::new();
        sources(&root, &mut files);
        assert!(files.len() > 50, "{}", root.display());
        let banned = [
            "EDGE_CAP",
            "edge_cap(",
            "MAX_GRAPH_EDGES",
            "edges_capped",
            "GraphEdgeCap",
            "source_items",
            "Budget::with_limits",
            "validate_with_limits",
            "read_strings_with_limits",
            "MAX_DECODED_",
            "MAX_GRAPH_BYTES",
        ];
        for file in files.iter().filter(|file| **file != root.join("limits.rs")) {
            let text = std::fs::read_to_string(file).expect("source");
            for name in banned {
                assert!(!text.contains(name), "{name} in {}", file.display());
            }
        }
    }
}
