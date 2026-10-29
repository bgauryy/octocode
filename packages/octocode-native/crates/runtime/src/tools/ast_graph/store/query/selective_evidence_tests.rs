//! Evidence pages over file-backed snapshots: lazy `STRE` blocks must match
//! the full decode, and only the blocks a page reads are authenticated.
use super::selective_tests::{ask, evidence, fixture, section};
use super::*;
use crate::execution::cancel::NeverCancel;
use crate::security::ContentSecurity;
use crate::tools::ast_graph::store::{IngestOptions, ingest};
use crate::tools::test_support::{Fixture, workspace_policy};
use std::path::PathBuf;

/// Flips the first body byte of every block of a blocked section.
pub(super) fn corrupt_blocks(original: &[u8], tag: &[u8; 4]) -> Vec<u8> {
    let (section_offset, _) = section(original, tag);
    let blocks = u32::from_le_bytes(
        original[section_offset + 4..section_offset + 8]
            .try_into()
            .unwrap(),
    ) as usize;
    let mut corrupted = original.to_vec();
    for block in 0..blocks {
        let entry = section_offset + 8 + block * 60;
        let body = u64::from_le_bytes(corrupted[entry + 8..entry + 16].try_into().unwrap());
        corrupted[section_offset + body as usize] ^= 0xff;
    }
    corrupted
}

#[test]
fn evidence_pages_match_full_decode_and_read_only_their_string_blocks() {
    let (dir, graph) = fixture();
    let bytes = std::fs::read(&graph).unwrap();
    let (stre, stre_len) = section(&bytes, b"STRE");
    let blocks = u32::from_le_bytes(bytes[stre + 4..stre + 8].try_into().unwrap());
    assert!(
        blocks > 2,
        "the fixture spans several evidence string blocks"
    );
    let mut ids = Vec::new();
    for (op, target) in [
        ("witnesses", "fn0000"),
        ("witnesses", "code.ts"),
        ("unresolved", "code.ts"),
    ] {
        for offset in (0..700).step_by(97) {
            let lazy = ask(&dir, op, target, offset, false);
            let full = ask(&dir, op, target, offset, true);
            assert_eq!(lazy.class, full.class, "{op}: {}", lazy.value);
            let validation = &lazy.value["validation"];
            assert_eq!(validation["scope"], "accessedBlocks", "{}", lazy.value);
            assert_eq!(validation["decodedEdges"], 0);
            assert!(
                validation["bytesRead"].as_u64().unwrap() + stre_len as u64 / 2
                    < bytes.len() as u64,
                "{op}/{offset} read most evidence text: {validation}"
            );
            if op == "witnesses" && target == "code.ts" {
                ids.extend(
                    lazy.value["results"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|row| row["id"].as_str().unwrap().to_owned()),
                );
            }
            assert_eq!(evidence(lazy), evidence(full), "{op}/{target}/{offset}");
        }
    }
    assert!(!ids.is_empty());
}

#[test]
fn evidence_pages_reject_corrupt_blocks_they_read_and_disclose_unread_ones() {
    let (dir, graph) = fixture();
    let original = std::fs::read(&graph).unwrap();
    let (stre, _) = section(&original, b"STRE");
    let blocks = u32::from_le_bytes(original[stre + 4..stre + 8].try_into().unwrap()) as usize;
    let last = stre + 8 + (blocks - 1) * 60;
    let body = u64::from_le_bytes(original[last + 8..last + 16].try_into().unwrap()) as usize;
    let mut untouched = original.clone();
    untouched[stre + body] ^= 0xff;
    std::fs::write(&graph, &untouched).unwrap();
    let page = ask(&dir, "witnesses", "fn0000", 0, false);
    assert_eq!(page.class, ExitClass::Success, "{}", page.value);
    assert!(
        page.value["validation"]["checkedBlocks"].as_u64().unwrap()
            < page.value["validation"]["totalBlocks"].as_u64().unwrap()
    );
    assert!(super::super::format::decode(&untouched).is_err());
    std::fs::write(&graph, corrupt_blocks(&original, b"STRE")).unwrap();
    let page = ask(&dir, "witnesses", "fn0000", 0, false);
    assert_eq!(page.value["errorCode"], "decode", "{}", page.value);
}

#[test]
fn argument_evidence_pages_match_full_decode() {
    let dir = Fixture::new();
    let source = (0..300)
        .map(|index| {
            format!(
                "function handler{index:03}() {{}}\nexport function run{index:03}() {{ sink(handler{index:03}, missing{index:03}); registry[key](handler{index:03}); }}\n"
            )
        })
        .collect::<String>();
    dir.write(
        "app/args.ts",
        format!("function sink(...values: unknown[]) {{}}\n{source}"),
    );
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
    let mut rendered = 0;
    for op in ["witnesses", "unresolved"] {
        for offset in (0..600).step_by(113) {
            let page = |full: bool| {
                let options = QueryOptions {
                    op: op.into(),
                    edges: vec!["passes".into()],
                    workspace: Some(dir.0.to_path_buf()),
                    offset: Some(offset),
                    limit: Some(5),
                    ..Default::default()
                };
                match run_with(&options, &workspace_policy(&dir.0), &NeverCancel, !full) {
                    Ok(out) | Err(out) => out,
                }
            };
            let (lazy, full) = (page(false), page(true));
            assert_eq!(lazy.class, full.class, "{op}: {}", lazy.value);
            assert_eq!(lazy.value["validation"]["scope"], "accessedBlocks");
            rendered += lazy.value["results"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row.get("recipient").is_some())
                .count();
            assert_eq!(evidence(lazy), evidence(full), "{op}/{offset}");
        }
    }
    assert!(
        rendered > 0,
        "argument evidence rendered from evidence blocks"
    );
}

/// A fixture whose unresolved sites render source origins.
fn unresolved_fixture() -> (Fixture, PathBuf) {
    let dir = Fixture::new();
    let source = (0..700)
        .map(|index| format!("export function fn{index:04}() {{ return missing{index:04}(); }}\n"))
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

#[test]
fn origin_pages_read_only_their_origin_blocks_and_match_full_decode() {
    let (dir, graph) = unresolved_fixture();
    let bytes = std::fs::read(&graph).unwrap();
    let (srco, srco_len) = section(&bytes, b"SRCO");
    let blocks = u32::from_le_bytes(bytes[srco + 4..srco + 8].try_into().unwrap());
    assert!(blocks > 2, "the fixture spans several origin blocks");
    for offset in (0..700).step_by(131) {
        let lazy = ask(&dir, "unresolved", "code.ts", offset, false);
        let full = ask(&dir, "unresolved", "code.ts", offset, true);
        assert_eq!(lazy.class, ExitClass::Success, "{}", lazy.value);
        assert_eq!(lazy.value["total"], 700, "{}", lazy.value);
        assert!(
            lazy.value["results"][0]["sourceOrigin"].is_object()
                || lazy.value["results"][0]["sourceOrigin"] == "identity",
            "{}",
            lazy.value
        );
        let read = lazy.value["validation"]["bytesRead"].as_u64().unwrap();
        assert!(
            read + srco_len as u64 / 2 < bytes.len() as u64,
            "page {offset} read most origin records: {read} of {}",
            bytes.len()
        );
        assert_eq!(evidence(lazy), evidence(full), "unresolved/{offset}");
    }
}

#[test]
fn origin_pages_reject_corrupt_origin_blocks_they_read() {
    let (dir, graph) = unresolved_fixture();
    let original = std::fs::read(&graph).unwrap();
    let (srco, _) = section(&original, b"SRCO");
    let blocks = u32::from_le_bytes(original[srco + 4..srco + 8].try_into().unwrap()) as usize;
    let last = srco + 8 + (blocks - 1) * 60;
    let body = u64::from_le_bytes(original[last + 8..last + 16].try_into().unwrap()) as usize;
    let mut untouched = original.clone();
    untouched[srco + body] ^= 0xff;
    std::fs::write(&graph, &untouched).unwrap();
    let page = ask(&dir, "unresolved", "code.ts", 0, false);
    assert_eq!(page.class, ExitClass::Success, "{}", page.value);
    assert!(super::super::format::decode(&untouched).is_err());
    std::fs::write(&graph, corrupt_blocks(&original, b"SRCO")).unwrap();
    let page = ask(&dir, "unresolved", "code.ts", 0, false);
    assert_eq!(page.value["errorCode"], "decode", "{}", page.value);
}
