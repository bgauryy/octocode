use super::*;
use crate::execution::cancel::NeverCancel;
use crate::security::ContentSecurity;
use crate::tools::ast_graph::store::{IngestOptions, ingest};
use crate::tools::test_support::{Fixture, workspace_policy};

pub(super) fn fixture() -> (Fixture, PathBuf) {
    let dir = Fixture::new();
    let source = (0..700)
        .map(|index| format!("export function fn{index:04}() {{ return fn0000(); }}\n"))
        .collect::<String>();
    dir.write("app/code.ts", &source);
    let out = ingest(
        &IngestOptions {
            path: dir.0.join("app"),
            workspace: Some(dir.0.to_path_buf()),
            ..Default::default()
        },
        &workspace_policy(&dir.0),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let graph = PathBuf::from(out.value["dir"].as_str().unwrap()).join(GRAPH_FILE);
    (dir, graph)
}

pub(super) fn ask(dir: &Fixture, op: &str, target: &str, offset: usize, full: bool) -> GraphOutput {
    let options = QueryOptions {
        op: op.into(),
        target: Some(target.into()),
        workspace: Some(dir.0.to_path_buf()),
        offset: Some(offset),
        limit: Some(7),
        check_freshness: true,
        include_provenance: true,
        ..Default::default()
    };
    match run_with(&options, &workspace_policy(&dir.0), &NeverCancel, !full) {
        Ok(out) | Err(out) => out,
    }
}

pub(super) fn evidence(mut output: GraphOutput) -> Value {
    let object = output.value.as_object_mut().unwrap();
    object.remove("freshness");
    output.value
}

#[test]
fn selective_point_queries_match_full_projection_across_record_blocks() {
    let (dir, _) = fixture();
    for (op, target, offset) in [
        ("node", "code.ts#fn0257", 0),
        ("node", "code.ts", 0),
        ("find", "fn", 255),
        ("find", "fn06", 3),
        ("candidates", "fn0500", 0),
        ("node", "missing", 0),
    ] {
        let point = ask(&dir, op, target, offset, false);
        let full = ask(&dir, op, target, offset, true);
        assert_eq!(point.class, full.class, "{op}: {}", point.value);
        if !point.validation.is_null() {
            assert_eq!(point.validation["scope"], "accessedBlocks");
            assert!(
                point.validation["bytesRead"].as_u64().unwrap()
                    < point.validation["snapshotBytes"].as_u64().unwrap()
            );
            assert!(point.validation["decodedNodes"].as_u64().unwrap() < 701);
            assert_eq!(point.validation["decodedEdges"], 0);
            assert_eq!(point.validation["decodedWitnesses"], 0);
        }
        assert_eq!(evidence(point), evidence(full), "{op}/{target}/{offset}");
    }
}

#[test]
fn selective_find_pages_reach_every_row_once() {
    let (dir, _) = fixture();
    let mut ids = Vec::new();
    for offset in (0..700).step_by(7) {
        let out = ask(&dir, "find", "fn", offset, false);
        assert_eq!(out.class, ExitClass::Success, "{}", out.value);
        assert_eq!(out.value["pagination"]["totalItems"], 700);
        ids.extend(
            out.value["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["id"].as_str().unwrap().to_owned()),
        );
    }
    assert_eq!(ids.len(), 700);
    assert_eq!(
        ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
        700
    );
    assert_eq!(ids.first().map(String::as_str), Some("code.ts#fn0000"));
    assert_eq!(ids.last().map(String::as_str), Some("code.ts#fn0699"));
}

#[test]
fn topology_skips_occurrence_bodies_and_point_freshness_keeps_inventory_checks() {
    let (dir, _) = fixture();
    for op in ["stats", "hubs", "callers", "callees"] {
        let selected = ask(&dir, op, "fn0000", 0, false);
        let full = ask(&dir, op, "fn0000", 0, true);
        assert_eq!(selected.class, full.class, "{}", selected.value);
        assert!(matches!(
            selected.class,
            ExitClass::Success | ExitClass::Empty
        ));
        assert_eq!(selected.validation["scope"], "accessedBlocks");
        assert_eq!(selected.validation["decodedWitnesses"], 0);
        assert!(selected.validation["decodedEdges"].as_u64().unwrap() > 0);
        assert_eq!(evidence(selected), evidence(full), "{op}");
    }
    assert_eq!(
        ask(&dir, "node", "fn0000", 0, false).value["freshness"]["fresh"],
        true
    );
    dir.write("app/added.ts", "export const added = 1;");
    let selected = ask(&dir, "node", "fn0000", 0, false);
    assert_eq!(selected.value["freshness"]["fresh"], false);
    assert_eq!(
        evidence(selected),
        evidence(ask(&dir, "node", "fn0000", 0, true))
    );
}

pub(super) fn section(bytes: &[u8], tag: &[u8; 4]) -> (usize, usize) {
    let count = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    for index in 0..count {
        let start = 48 + index * 52;
        if &bytes[start..start + 4] == tag {
            return (
                u64::from_le_bytes(bytes[start + 4..start + 12].try_into().unwrap()) as usize,
                u64::from_le_bytes(bytes[start + 12..start + 20].try_into().unwrap()) as usize,
            );
        }
    }
    panic!("section absent");
}

#[test]
fn selective_integrity_discloses_unread_corruption_and_rejects_requested_blocks() {
    let (dir, graph) = fixture();
    let original = std::fs::read(&graph).unwrap();
    let (offset, len) = section(&original, b"WITN");
    let mut corrupted = original.clone();
    corrupted[offset + len - 1] ^= 0xff;
    std::fs::write(&graph, &corrupted).unwrap();
    let point = ask(&dir, "node", "fn0000", 0, false);
    assert_eq!(point.class, ExitClass::Success, "{}", point.value);
    assert!(
        point.validation["checkedBlocks"].as_u64().unwrap()
            < point.validation["totalBlocks"].as_u64().unwrap()
    );
    assert_ne!(
        ask(&dir, "witnesses", "fn0000", 0, false).class,
        ExitClass::Success
    );
    assert!(super::super::format::decode(&corrupted).is_err());

    let (node, _) = section(&original, b"NODE");
    let block_count = u32::from_le_bytes(original[node + 4..node + 8].try_into().unwrap()) as usize;
    for block in [0, block_count - 1] {
        let entry = node + 8 + block * 60;
        let body = u64::from_le_bytes(original[entry + 8..entry + 16].try_into().unwrap()) as usize;
        let mut corrupted = original.clone();
        corrupted[node + body] ^= 0xff;
        std::fs::write(&graph, &corrupted).unwrap();
        let point = ask(&dir, "node", "fn0000", 0, false);
        assert_eq!(
            point.class == ExitClass::Success,
            block != 0,
            "{}",
            point.value
        );
        assert!(super::super::format::decode(&corrupted).is_err());
    }
    let mut corrupted = original;
    corrupted[48 + 20] ^= 1;
    std::fs::write(&graph, corrupted).unwrap();
    assert_ne!(
        ask(&dir, "node", "fn0000", 0, false).class,
        ExitClass::Success
    );
}

fn corrupt_section(original: &[u8], tag: &[u8; 4]) -> Vec<u8> {
    let (offset, len) = section(original, tag);
    let mut corrupted = original.to_vec();
    corrupted[offset + len - 1] ^= 0xff;
    corrupted
}

#[test]
fn point_and_topology_reads_skip_evidence_text() {
    let (dir, graph) = fixture();
    let bytes = std::fs::read(&graph).unwrap();
    let len = |tag: &[u8; 4]| section(&bytes, tag).1 as u64;
    // Opening a reader authenticates the NODE, DECL, STRE and SRCO block
    // directories; a topology read then reads NODE and DECL whole.
    let directories = [b"NODE", b"DECL", b"STRE", b"SRCO"]
        .into_iter()
        .map(|tag| {
            let (offset, _) = section(&bytes, tag);
            let blocks = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
            8 + 60 * u64::from(blocks)
        })
        .sum::<u64>();
    let unread = len(b"STRE") + len(b"WITN") + len(b"UNRS");
    assert!(len(b"STRE") > 0, "occurrence ids are evidence-only text");
    for (op, target) in [
        ("node", "code.ts#fn0257"),
        ("node", "code.ts"),
        ("find", "fn06"),
        ("callers", "fn0000"),
        ("stats", "fn0000"),
    ] {
        let out = ask(&dir, op, target, 0, false);
        assert_eq!(out.class, ExitClass::Success, "{op}: {}", out.value);
        let read = out.validation["bytesRead"].as_u64().unwrap();
        assert!(
            read <= bytes.len() as u64 + directories - unread,
            "{op} read {read} of {} bytes",
            bytes.len()
        );
    }
    let file = ask(&dir, "node", "code.ts", 0, false);
    assert!(
        file.validation["bytesRead"].as_u64().unwrap()
            <= bytes.len() as u64 - unread - len(b"SRCE"),
        "a file node renders no source origin"
    );
}

#[test]
fn unread_evidence_text_and_sources_cannot_fail_lookups_that_skip_them() {
    let (dir, graph) = fixture();
    let original = std::fs::read(&graph).unwrap();
    let clean_miss = ask(&dir, "node", "missing", 0, false);
    let clean_candidates = ask(&dir, "candidates", "missing", 0, false).class;
    let clean_callers = evidence(ask(&dir, "callers", "fn0000", 0, false));
    for tag in [b"STRE", b"SRCE"] {
        let corrupted = corrupt_section(&original, tag);
        std::fs::write(&graph, &corrupted).unwrap();
        let miss = ask(&dir, "node", "missing", 0, false);
        assert_eq!(miss.class, clean_miss.class, "{}", miss.value);
        assert_eq!(miss.value["errorCode"], clean_miss.value["errorCode"]);
        assert_eq!(
            ask(&dir, "candidates", "missing", 0, false).class,
            clean_candidates
        );
        assert_eq!(
            ask(&dir, "node", "code.ts", 0, false).class,
            ExitClass::Success
        );
        // Whatever a result renders is still authenticated before use.
        let reader = if tag == b"STRE" {
            std::fs::write(
                &graph,
                super::selective_evidence_tests::corrupt_blocks(&original, b"STRE"),
            )
            .unwrap();
            ask(&dir, "witnesses", "fn0000", 0, false)
        } else {
            ask(&dir, "node", "code.ts#fn0257", 0, false)
        };
        assert_ne!(reader.class, ExitClass::Success, "{}", reader.value);
        assert!(super::super::format::decode(&corrupted).is_err());
    }
    std::fs::write(&graph, corrupt_section(&original, b"STRE")).unwrap();
    assert_eq!(
        evidence(ask(&dir, "callers", "fn0000", 0, false)),
        clean_callers
    );
}

#[test]
fn deferred_source_read_observes_cancellation() {
    struct Cancelled;
    impl crate::execution::cancel::CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            Err("cancelled".into())
        }
    }
    let (dir, _) = fixture();
    let options = QueryOptions {
        op: "node".into(),
        target: Some("code.ts#fn0257".into()),
        workspace: Some(dir.0.to_path_buf()),
        ..Default::default()
    };
    let graph = load(&options, &workspace_policy(&dir.0), &NeverCancel).unwrap();
    let error = graph.origin(0, &Cancelled).unwrap_err();
    assert_eq!(error.value["errorCode"], "cancelled", "{}", error.value);
}
