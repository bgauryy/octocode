//! Every graph build and snapshot bound, owned once. A build is refused as
//! soon as its snapshot can no longer publish; writers take [`Limits`]
//! (tests lower them), readers decode untrusted snapshots under [`LIMITS`].
//!
//! The topology a query loads whole (NODE and EDGE rows, their indexes,
//! adjacency and strings) is bounded by the process memory budget
//! ([`octocode_engine::memory::MEMORY`]), priced per row by
//! [`NODE_ROW_BYTES`] and [`EDGE_ROW_BYTES`]. What a build itself holds
//! (every kept file's facts, source correspondence and projected tables) is
//! bounded by [`build_budget`], charged per file by [`build_charge`].

mod charges;

#[cfg(test)]
pub(crate) use charges::read_charge;
pub(crate) use charges::{ORIGIN_INDEX_BYTES, build_budget, build_charge};

/// The bounds one graph build and its snapshot obey.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Limits {
    /// `graph.bin` bytes, written and read.
    pub snapshot_bytes: usize,
    /// Rows per snapshot table but EDGE: the NODE rows the memory budget
    /// holds on their own.
    pub table_items: usize,
    /// EDGE rows the memory budget holds on their own. The build tallies
    /// its certain edges against it, so a build that passes it can still
    /// publish.
    pub edges: usize,
    /// Loaded topology bytes ([`NODE_ROW_BYTES`] per node plus
    /// [`EDGE_ROW_BYTES`] per edge) one snapshot may need: the budget its
    /// NODE and EDGE rows share.
    pub topology_bytes: usize,
    /// Expanded bytes of both string dictionaries (STRS and STRE).
    pub string_bytes: usize,
    /// Allocation charge of the source catalog (SRCE) and of each block of
    /// the origin pool (SRCO): a reader admits origins a block at a time.
    pub source_bytes: usize,
    /// Mapping work of the source catalog and of each origin block.
    pub source_work: usize,
    /// Checkpoint artifacts on disk.
    pub spill_bytes: u64,
    /// Artifacts held in memory because their sources hold secrets.
    pub memory_only_bytes: u64,
    /// Encoded bytes of the artifacts a checkpointed build keeps decoded
    /// for linking; past it, linking reads them back from disk.
    pub retained_bytes: u64,
    /// One extraction artifact.
    pub artifact_bytes: usize,
    /// Bytes one build may hold at its peak ([`build_budget`]): the scan
    /// charges each kept file [`build_charge`] and refuses past it.
    pub build_bytes: usize,
}

/// Bytes one NODE row and one EDGE row hold once a query loads a snapshot's
/// topology (`graphQuery hubs`: tables, lookup indexes, both adjacency
/// directions and the record strings). Measured 2026-11-06 on release
/// builds as the peak memory a fresh server added to load four snapshots,
/// least-squares fit `bytes = 425·nodes + 64·edges + 11 MB`, within 6% of
/// each: prometheus (18,083 nodes, 59,388 edges: 21.3 MB), kubernetes `pkg`
/// (50,865, 137,440: 42.9 MB), vscode (335,573, 1,169,637: 228.3 MB) and
/// linux `drivers/gpu` (1,254,328, 1,483,601: 638.6 MB).
pub(crate) const NODE_ROW_BYTES: usize = 425;
/// See [`NODE_ROW_BYTES`].
pub(crate) const EDGE_ROW_BYTES: usize = 64;

/// The bounds of this process: its topology bounds from the memory budget,
/// its build bound from the host's memory.
pub(crate) static LIMITS: std::sync::LazyLock<Limits> = std::sync::LazyLock::new(|| {
    Limits::for_memory(octocode_engine::memory::MEMORY.limit(), build_budget())
});

/// The bounds that do not depend on the host.
const FIXED: Limits = Limits {
    snapshot_bytes: 512 << 20,
    table_items: 0,
    edges: 0,
    topology_bytes: 0,
    string_bytes: 512 << 20,
    source_bytes: 512 << 20,
    source_work: 512 << 20,
    spill_bytes: 2 << 30,
    memory_only_bytes: 64 << 20,
    retained_bytes: 128 << 20,
    artifact_bytes: 64 << 20,
    build_bytes: 0,
};

/// Sections one snapshot may hold (format shape, read side).
pub(crate) const MAX_SECTIONS: usize = 64;

// The fixed bounds agree, or the crate does not build: no part of a
// snapshot may outgrow the whole.
const _: () = {
    assert!(FIXED.source_bytes <= FIXED.snapshot_bytes);
    assert!(FIXED.string_bytes <= FIXED.snapshot_bytes);
    assert!(FIXED.artifact_bytes as u64 <= FIXED.spill_bytes);
    assert!(FIXED.memory_only_bytes <= FIXED.spill_bytes);
};

impl Default for Limits {
    fn default() -> Self {
        *LIMITS
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

/// How a tally overfills the snapshot, worded as the clause after "they".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Overfill {
    /// One table's rows past its bound.
    Rows { table: &'static str, bound: usize },
    /// The loaded topology past the memory budget.
    Memory { needed: usize, budget: usize },
    /// What the build holds past its budget ([`build_budget`]).
    BuildMemory { needed: usize, budget: usize },
}

impl std::fmt::Display for Overfill {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rows { table, bound } => {
                write!(f, "project more than the snapshot's {bound} {table} rows")
            }
            Self::Memory { needed, budget } => write!(
                f,
                "need about {} MiB to load their topology, past this host's {} MiB memory budget",
                needed >> 20,
                budget >> 20
            ),
            Self::BuildMemory { needed, budget } => write!(
                f,
                "need about {} MiB to build, past this host's {} MiB build budget (half its memory)",
                needed >> 20,
                budget >> 20
            ),
        }
    }
}

impl Limits {
    /// The bounds of a process whose memory budget is `budget` bytes and
    /// whose builds may hold `build` bytes: the rows of each topology table
    /// that budget holds on its own, the budget their rows share, and the
    /// build's own bound.
    pub(crate) fn for_memory(budget: usize, build: usize) -> Self {
        Self {
            table_items: budget / NODE_ROW_BYTES,
            edges: budget / EDGE_ROW_BYTES,
            topology_bytes: budget,
            build_bytes: build,
            ..FIXED
        }
    }

    /// The bounds of one file's source correspondence in a build: its bytes
    /// are the build's memory budget, which every kept file's
    /// correspondence is then charged against ([`build_charge`]), so only a
    /// file that no build could hold is declined, naming that budget. The
    /// map's structural bounds are its own.
    pub(crate) fn source_map(&self) -> octocode_engine::text::analysis::MapLimits {
        octocode_engine::text::analysis::MapLimits {
            bytes: self.build_bytes,
            bytes_budget: "build memory budget",
            ..Default::default()
        }
    }

    /// How a build whose kept files charge `charged` bytes passes its
    /// budget; a charge exactly at it builds.
    pub(crate) fn admit_build(&self, charged: usize) -> Result<(), Overfill> {
        if charged > self.build_bytes {
            return Err(Overfill::BuildMemory {
                needed: charged,
                budget: self.build_bytes,
            });
        }
        Ok(())
    }

    /// The EDGE rows a reader admits.
    pub(crate) fn edge_rows(&self) -> usize {
        self.edges
    }

    /// How `tally` already overfills the snapshot: a table past its rows,
    /// or both past the memory they share. A tally exactly at a bound
    /// publishes.
    pub(crate) fn admit(&self, tally: &Tally) -> Result<(), Overfill> {
        if tally.nodes > self.table_items {
            return Err(Overfill::Rows {
                table: "NODE",
                bound: self.table_items,
            });
        }
        if tally.edges > self.edge_rows() {
            return Err(Overfill::Rows {
                table: "EDGE",
                bound: self.edge_rows(),
            });
        }
        let needed = tally
            .nodes
            .saturating_mul(NODE_ROW_BYTES)
            .saturating_add(tally.edges.saturating_mul(EDGE_ROW_BYTES));
        if needed > self.topology_bytes {
            return Err(Overfill::Memory {
                needed,
                budget: self.topology_bytes,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::charges::{DECLARATION_BUILD_BYTES, UNKNOWN_HOST_MB, build_budget_for};
    use super::*;

    /// The build's bounds can never pass what the reader admits, and the
    /// topology bounds come from the memory budget: each table alone fits
    /// it, and so does the topology it holds.
    #[test]
    fn limits_are_consistent() {
        assert_eq!(LIMITS.edge_rows(), LIMITS.edges);
        assert_eq!(
            Limits::default(),
            *LIMITS,
            "writers default to the reader's bounds"
        );
        let budget = octocode_engine::memory::MEMORY.limit();
        assert_eq!(*LIMITS, Limits::for_memory(budget, build_budget()));
        assert!(LIMITS.table_items * NODE_ROW_BYTES <= budget);
        assert!(LIMITS.edges * EDGE_ROW_BYTES <= budget);
        assert_eq!(LIMITS.topology_bytes, budget);
    }

    #[test]
    fn a_tally_at_a_bound_is_admitted_and_one_past_it_names_the_table() {
        let limits = Limits {
            table_items: 4,
            edges: 3,
            ..*LIMITS
        };
        assert_eq!(limits.admit(&Tally { nodes: 4, edges: 3 }), Ok(()));
        assert_eq!(
            limits.admit(&Tally { nodes: 5, edges: 0 }),
            Err(Overfill::Rows {
                table: "NODE",
                bound: 4
            })
        );
        assert_eq!(
            limits.admit(&Tally { nodes: 1, edges: 4 }),
            Err(Overfill::Rows {
                table: "EDGE",
                bound: 3
            })
        );
        // Rows each table holds alone can still overfill the memory they
        // share: the budget for exactly four nodes and three edges.
        let shared = Limits {
            topology_bytes: 4 * NODE_ROW_BYTES + 3 * EDGE_ROW_BYTES,
            ..limits
        };
        let shared = Limits {
            table_items: 10,
            edges: 10,
            ..shared
        };
        assert_eq!(shared.admit(&Tally { nodes: 4, edges: 3 }), Ok(()));
        assert_eq!(
            shared.admit(&Tally { nodes: 5, edges: 3 }),
            Err(Overfill::Memory {
                needed: 5 * NODE_ROW_BYTES + 3 * EDGE_ROW_BYTES,
                budget: shared.topology_bytes
            })
        );
        let mut tally = Tally::default();
        tally.add(Tally::file(2));
        tally.add(Tally::file(0));
        assert_eq!(tally, Tally { nodes: 4, edges: 2 });
    }

    /// A build may hold half of the host's memory, read where the rest of
    /// the runtime reads it; an unreadable host counts as [`UNKNOWN_HOST_MB`].
    #[test]
    fn the_build_budget_is_half_the_host_memory() {
        assert_eq!(LIMITS.build_bytes, build_budget());
        assert_eq!(
            build_budget(),
            build_budget_for(octocode_engine::host::memory_mb())
        );
        assert_eq!(build_budget_for(Some(16 * 1024)), 8 << 30);
        assert_eq!(build_budget_for(Some(12 * 1024)), 6 << 30);
        assert_eq!(
            build_budget_for(None),
            build_budget_for(Some(UNKNOWN_HOST_MB))
        );
    }

    /// A charge at the build budget builds; one byte past it is refused,
    /// worded with both sizes.
    #[test]
    fn a_charge_past_the_build_budget_is_refused_with_both_sizes() {
        let limits = Limits {
            build_bytes: 3 << 20,
            ..*LIMITS
        };
        assert_eq!(limits.admit_build(3 << 20), Ok(()));
        let over = limits
            .admit_build((3 << 20) + 1)
            .expect_err("past the budget");
        assert_eq!(
            over,
            Overfill::BuildMemory {
                needed: (3 << 20) + 1,
                budget: 3 << 20
            }
        );
        let text = over.to_string();
        assert!(text.contains("3 MiB build budget"), "{text}");
        // A file is charged at least its encoded facts, a declaration's
        // bytes per declaration, and its correspondence in full.
        assert!(build_charge(400, 0, 0) >= 400);
        assert!(build_charge(0, 1, 0) >= DECLARATION_BUILD_BYTES);
        assert_eq!(build_charge(400, 1, 7), build_charge(400, 1, 0) + 7);
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
