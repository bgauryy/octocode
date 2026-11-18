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
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ast_graph");
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
    // The limits module itself, its tests (this list) included, owns them.
    let owner = |file: &std::path::Path| {
        file == root.join("limits.rs") || file.starts_with(root.join("limits"))
    };
    for file in files.iter().filter(|file| !owner(file)) {
        let text = std::fs::read_to_string(file).expect("source");
        for name in banned {
            assert!(!text.contains(name), "{name} in {}", file.display());
        }
    }
}
