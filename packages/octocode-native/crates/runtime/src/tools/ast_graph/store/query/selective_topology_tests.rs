use super::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, workspace_policy};
use selective_tests::{ask, fixture, section};

#[test]
fn topology_reads_only_selected_declaration_and_source_blocks() {
    let (dir, graph) = fixture();
    let bytes = std::fs::read(graph).unwrap();
    for op in ["hubs", "cycles", "stale", "diagnostics"] {
        let out = ask(&dir, op, "code.ts", 0, false);
        assert!(matches!(out.class, ExitClass::Success | ExitClass::Empty));
        assert_eq!(out.validation["declarationBlocksRead"], 0, "{op}");
        assert_eq!(out.validation["sourceBlocksRead"], 0, "{op}");
    }
    for op in ["stats", "callers"] {
        let out = ask(&dir, op, "fn0000", 255, false);
        assert_eq!(out.class, ExitClass::Success, "{}", out.value);
        for (tag, metric) in [
            (b"DECL", "declarationBlocksRead"),
            (b"SRCO", "sourceBlocksRead"),
        ] {
            let (offset, _) = section(&bytes, tag);
            let blocks = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
            let read = out.validation[metric].as_u64().unwrap();
            assert!(
                read > 0 && read < u64::from(blocks),
                "{op}/{metric}: {read}/{blocks}"
            );
        }
        if op == "stats" {
            assert_eq!(out.value["mostCalledSymbols"][0]["id"], "code.ts#fn0000");
            assert!(out.value["mostCalledSymbols"][0]["sourceOrigin"].is_object());
        } else {
            assert_eq!(out.value["rows"].as_array().unwrap().len(), 7);
        }
    }
}

fn query(dir: &Fixture, graph: &Path, operation: &str, decode: Decode) -> GraphOutput {
    let options = QueryOptions {
        op: operation.into(),
        target: Some(
            if operation == "symbols" {
                "code.ts"
            } else {
                "fn0000"
            }
            .into(),
        ),
        to: Some("fn0000".into()),
        baseline: Some(graph.parent().unwrap().to_string_lossy().into_owned()),
        workspace: Some(dir.0.to_path_buf()),
        include_provenance: true,
        limit: Some(7),
        ..Default::default()
    };
    match run_with(&options, &workspace_policy(&dir.0), &NeverCancel, decode) {
        Ok(out) | Err(out) => out,
    }
}

fn evidence(out: GraphOutput) -> Value {
    out.value
}

#[test]
fn every_topology_operation_matches_the_full_snapshot_oracle() {
    let (dir, graph) = fixture();
    for operation in OPS.iter().copied().filter(|op| {
        !matches!(
            *op,
            "node" | "find" | "candidates" | "witnesses" | "unresolved"
        )
    }) {
        let selective = query(&dir, &graph, operation, Decode::Selective);
        let full = query(&dir, &graph, operation, Decode::Full);
        assert_eq!(
            selective.class, full.class,
            "{operation}: {}",
            selective.value
        );
        assert!(
            matches!(selective.class, ExitClass::Success | ExitClass::Empty),
            "{operation}: {}",
            selective.value
        );
        assert_eq!(evidence(selective), evidence(full), "{operation}");
    }
}

#[test]
fn topology_checks_required_tables_and_every_source_block_it_renders() {
    let (dir, graph) = fixture();
    let original = std::fs::read(&graph).unwrap();
    for tag in [b"DECL", b"SRCO"] {
        let corrupted = selective_evidence_tests::corrupt_blocks(&original, tag);
        std::fs::write(&graph, corrupted).unwrap();
        let hubs = ask(&dir, "hubs", "code.ts", 0, false);
        assert!(
            matches!(hubs.class, ExitClass::Success | ExitClass::Empty),
            "{tag:?}: {}",
            hubs.value
        );
        for op in ["stats", "callers"] {
            let selected = ask(&dir, op, "fn0000", 255, false);
            assert_eq!(
                selected.value["errorCode"], "decode",
                "{tag:?}/{op}: {}",
                selected.value
            );
        }
    }
    for tag in [b"NODE", b"EDGE"] {
        let (offset, length) = section(&original, tag);
        let mut corrupted = original.clone();
        corrupted[offset + length - 1] ^= 0xff;
        std::fs::write(&graph, corrupted).unwrap();
        let selected = ask(&dir, "stats", "code.ts", 0, false);
        assert_eq!(
            selected.value["errorCode"], "decode",
            "{tag:?}: {}",
            selected.value
        );
    }
}
