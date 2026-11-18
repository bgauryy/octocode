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
    /// file that no build could hold is declined, naming that budget. Its
    /// volume counts (segments, branches, contributors) are not bounded on
    /// their own: each item's memory is charged to those bytes, and a count
    /// sized for one source map would cut a dense file of many spans. Its
    /// time is the request's deadline and cancellation, which every unit of
    /// work checks. Only the map's shape bounds (nesting depth,
    /// representations) stay the map's own.
    pub(crate) fn source_map(&self) -> octocode_engine::text::analysis::MapLimits {
        octocode_engine::text::analysis::MapLimits {
            bytes: self.build_bytes,
            bytes_budget: "build memory budget",
            segments: usize::MAX,
            branches: usize::MAX,
            contributors: usize::MAX,
            work: usize::MAX,
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
mod tests;
