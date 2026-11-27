use super::*;
use crate::ast_graph::store::{IngestOptions, ingest};
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::security::ContentSecurity;
use octocode_runtime_base::test_support::{Fixture, workspace_policy};
use selective_tests::{ask, fixture, section};

#[test]
fn topology_reads_only_selected_declaration_and_source_blocks() {
    let (dir, graph) = fixture();
    let bytes = std::fs::read(graph).unwrap();
    // `stats` states the manifest's counts: no node row, so no block.
    for op in ["stats", "hubs", "cycles", "stale", "diagnostics"] {
        let out = ask(&dir, op, "code.ts", 0, false);
        assert!(matches!(out.class, ExitClass::Success | ExitClass::Empty));
        assert_eq!(out.validation["declarationBlocksRead"], 0, "{op}");
        assert_eq!(out.validation["sourceBlocksRead"], 0, "{op}");
    }
    let out = ask(&dir, "callers", "fn0000", 255, false);
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
            "callers/{metric}: {read}/{blocks}"
        );
    }
    assert_eq!(out.value["rows"].as_array().unwrap().len(), 7);
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
        // `cochange` reads Git history, which this fixture has none of: both
        // decodes refuse it alike.
        assert!(
            operation == "cochange"
                || matches!(selective.class, ExitClass::Success | ExitClass::Empty),
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
        // `stats` reads no block; a row answer reads the damaged one.
        let selected = ask(&dir, "callers", "fn0000", 255, false);
        assert_eq!(
            selected.value["errorCode"], "snapshotIncompatible",
            "{tag:?}/callers: {}",
            selected.value
        );
    }
    for tag in [b"NODE", b"EDGE"] {
        let (offset, length) = section(&original, tag);
        let mut corrupted = original.clone();
        corrupted[offset + length - 1] ^= 0xff;
        std::fs::write(&graph, corrupted).unwrap();
        let selected = ask(&dir, "stats", "code.ts", 0, false);
        assert_eq!(
            selected.value["errorCode"], "snapshotIncompatible",
            "{tag:?}: {}",
            selected.value
        );
    }
}

/// Rewrites section `tag` of `original` in place with `edit` (same length)
/// and recomputes every digest that covers it: the section's directory
/// entry, the directory digest in the header and the manifest's. The result
/// is a crafted snapshot that passes every checksum.
fn reseal(graph: &Path, original: &[u8], tag: &[u8; 4], edit: impl FnOnce(&mut [u8])) {
    let mut bytes = original.to_vec();
    let (offset, len) = section(&bytes, tag);
    edit(&mut bytes[offset..offset + len]);
    let digest = octocode_engine::digest::sha256_digest(&bytes[offset..offset + len]);
    let count = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let entry = (0..count)
        .map(|index| 48 + index * 52)
        .find(|entry| &bytes[*entry..*entry + 4] == tag)
        .unwrap();
    bytes[entry + 20..entry + 52].copy_from_slice(&digest);
    let directory = octocode_engine::digest::sha256_digest(&bytes[48..48 + count * 52]);
    bytes[16..48].copy_from_slice(&directory);
    std::fs::write(graph, &bytes).unwrap();
    let manifest_path = graph.parent().unwrap().join(MANIFEST_FILE);
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["graph"]["sha256"] = json!(hex::encode(directory));
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
}

/// The byte after the varint at `at`.
fn varint_end(bytes: &[u8], mut at: usize) -> usize {
    while bytes[at] & 0x80 != 0 {
        at += 1;
    }
    at + 1
}

/// The `LOOK` entries' byte ranges: kind, key, name, then 12 degrees.
fn lookup_entries(look: &[u8]) -> Vec<(usize, usize)> {
    let mut at = varint_end(look, 0);
    let mut entries = Vec::new();
    while at < look.len() {
        let start = at;
        at += 1;
        for _ in 0..14 {
            at = varint_end(look, at);
        }
        entries.push((start, at));
    }
    entries
}

fn answers(dir: &Fixture) -> Vec<(&'static str, Value)> {
    [
        ("callers", "fn0000"),
        ("callees", "fn0257"),
        ("stats", "code.ts"),
        ("hubs", "code.ts"),
    ]
    .into_iter()
    .map(|(op, target)| {
        let out = ask(dir, op, target, 0, false);
        assert!(
            matches!(out.class, ExitClass::Success | ExitClass::Empty),
            "{op}: {}",
            out.value
        );
        (op, selective_tests::evidence(out))
    })
    .collect()
}

#[test]
fn a_topology_answer_reads_no_lookup_entry_and_is_not_refused_by_one() {
    let (dir, graph) = fixture();
    let original = std::fs::read(&graph).unwrap();
    let expected = answers(&dir);
    // Every node's first incoming degree, off by one: the lookup no longer
    // agrees with the edges, and every checksum still matches.
    reseal(&graph, &original, b"LOOK", bump_degrees);
    let crafted = std::fs::read(&graph).unwrap();
    // The writer's round trip (the full decode) still cross-checks the
    // lookup against the topology.
    assert_eq!(
        super::super::format::decode(&crafted).unwrap_err(),
        "graph lookup degree differs from topology"
    );
    // A topology answer renders nodes and edges, never a lookup entry.
    assert_eq!(answers(&dir), expected);
}

#[test]
fn a_crafted_entry_a_query_reads_is_still_refused() {
    let (dir, graph) = fixture();
    let original = std::fs::read(&graph).unwrap();
    // The name index a topology lookup binary-searches, two entries swapped.
    reseal(&graph, &original, b"NAMX", |namx| {
        let mut at = varint_end(namx, 0);
        let mut ids = Vec::new();
        while at < namx.len() {
            let end = varint_end(namx, at);
            ids.push((at, end));
            at = end;
        }
        let pair = ids
            .windows(2)
            .find(|pair| pair[0].1 - pair[0].0 == pair[1].1 - pair[1].0)
            .unwrap();
        let ((a, a_end), (b, b_end)) = (pair[0], pair[1]);
        let first = namx[a..a_end].to_vec();
        namx.copy_within(b..b_end, a);
        namx[a + (b_end - b)..b_end].copy_from_slice(&first);
    });
    for op in ["callers", "find"] {
        let out = ask(&dir, op, "fn0000", 0, false);
        assert_eq!(
            out.value["errorCode"], "snapshotIncompatible",
            "{op}: {}",
            out.value
        );
        assert!(
            out.value["error"]
                .as_str()
                .is_some_and(|error| error.contains("graph lookup indexes are not sorted")),
            "{op}: {}",
            out.value
        );
    }
    // Every symbol's lookup entry named a package: a point answer reads the
    // lookup entry and the node record behind it, and refuses the pair.
    reseal(&graph, &original, b"LOOK", |look| {
        for (start, _) in lookup_entries(look) {
            if look[start] == NodeKind::Symbol as u8 {
                look[start] = NodeKind::Package as u8;
            }
        }
    });
    let out = ask(&dir, "node", "code.ts#fn0257", 0, false);
    assert_eq!(
        out.value["errorCode"], "snapshotIncompatible",
        "{}",
        out.value
    );
    assert!(
        out.value["error"]
            .as_str()
            .is_some_and(|error| error.contains("graph node differs from its lookup")),
        "{}",
        out.value
    );
}

/// Every node's first incoming degree in `LOOK`, off by one.
fn bump_degrees(look: &mut [u8]) {
    for (start, _) in lookup_entries(look) {
        let mut at = start + 1;
        for _ in 0..2 {
            at = varint_end(look, at);
        }
        assert!(look[at] < 0x7f, "a one-byte degree");
        look[at] += 1;
    }
}

fn varint(mut value: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

/// A topology answer never reads the lookup, but a reused snapshot serves
/// point answers too, and they render its degrees: reuse proves the lookup
/// against the topology and rebuilds a snapshot whose lookup disagrees.
#[test]
fn a_snapshot_whose_lookup_disagrees_with_its_topology_is_rebuilt_not_reused() {
    let (dir, graph) = fixture();
    let original = std::fs::read(&graph).unwrap();
    let id = graph
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let degrees = |out: GraphOutput| {
        (
            out.value["node"]["in"].clone(),
            out.value["node"]["out"].clone(),
        )
    };
    let expected = degrees(ask(&dir, "node", "code.ts#fn0257", 0, false));
    reseal(&graph, &original, b"LOOK", bump_degrees);
    let crafted = degrees(ask(&dir, "node", "code.ts#fn0257", 0, false));
    assert_ne!(
        crafted, expected,
        "the point answer renders the lookup's degrees"
    );
    let options = IngestOptions {
        path: dir.0.join("app"),
        workspace: Some(dir.0.to_path_buf()),
        ..Default::default()
    };
    let out = ingest(
        &options,
        &workspace_policy(&dir.0),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    assert_ne!(
        out.value["graph"],
        json!(id),
        "rebuilt, not reused: {}",
        out.value
    );
    assert_eq!(
        degrees(ask(&dir, "node", "code.ts#fn0257", 0, false)),
        expected
    );
    // An unchanged, consistent snapshot is still reused.
    let again = ingest(
        &options,
        &workspace_policy(&dir.0),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(again.value["graph"], out.value["graph"], "{}", again.value);
}

/// Reseals `node`'s `LOOK` key as another record string that keeps the key
/// order and reads as a path: a lookup entry that misstates its node.
fn misstate_lookup_key(graph: &Path, original: &[u8], node: usize) {
    let (records, _) = super::super::format::decode_files(original).unwrap();
    let (t, _) = super::super::format::decode(original).unwrap();
    let key = t.nodes[node].key as usize;
    let position = t
        .key_index
        .iter()
        .position(|id| *id as usize == node)
        .unwrap();
    let below = position
        .checked_sub(1)
        .map_or(0, |at| t.nodes[t.key_index[at] as usize].key as usize + 1);
    let above = t
        .key_index
        .get(position + 1)
        .map_or(records.strings.len(), |id| {
            t.nodes[*id as usize].key as usize
        });
    let other = (below..above)
        .find(|id| {
            *id != key
                && varint(*id).len() == varint(key).len()
                && !records.strings[*id].is_empty()
                && !records.strings[*id].contains(['/', '.'])
                && t.nodes.iter().all(|node| node.key as usize != *id)
        })
        .expect("a substitute key");
    reseal(graph, original, b"LOOK", |look| {
        let (start, _) = lookup_entries(look)[node];
        let encoded = varint(other);
        look[start + 1..start + 1 + encoded.len()].copy_from_slice(&encoded);
    });
}

fn assert_misstated(out: &GraphOutput) {
    assert_eq!(
        out.value["errorCode"], "snapshotIncompatible",
        "{}",
        out.value
    );
    assert!(
        out.value["error"]
            .as_str()
            .is_some_and(|error| error.contains("graph node differs from its lookup")),
        "{}",
        out.value
    );
}

/// A point answer renders a symbol's file key from the file's lookup entry:
/// that entry is checked against the file's node record first.
#[test]
fn a_point_answer_refuses_a_file_key_its_lookup_entry_misstates() {
    let (dir, graph) = fixture();
    let original = std::fs::read(&graph).unwrap();
    let (t, _) = super::super::format::decode(&original).unwrap();
    let file = t
        .nodes
        .iter()
        .position(|node| node.kind == NodeKind::File && t.str(node.key) == "code.ts")
        .unwrap();
    misstate_lookup_key(&graph, &original, file);
    assert_misstated(&ask(&dir, "node", "code.ts#fn0257", 0, false));
}

/// A point `node` answer renders a method's parent key from the parent's
/// lookup entry: that entry is checked against the parent's node record.
#[test]
fn a_point_answer_refuses_a_parent_key_its_lookup_entry_misstates() {
    // A `NODE` block holds 256 records, and decoding a block checks each of
    // its entries. The functions put the class in another block than the
    // file, and the methods put the last one in a third, so only the
    // parent's own check reads the parent's block.
    let dir = Fixture::new();
    let functions = (0..300)
        .map(|index| format!("export function f{index:03}() {{ return 0; }}\n"))
        .collect::<String>();
    let methods = (0..300)
        .map(|index| format!("  m{index:03}() {{ return 1; }}\n"))
        .collect::<String>();
    dir.write(
        "app/box.ts",
        format!("{functions}export class Box {{\n{methods}}}\n"),
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
    let graph = PathBuf::from(out.value["dir"].as_str().unwrap()).join(GRAPH_FILE);
    let original = std::fs::read(&graph).unwrap();
    let method = ask(&dir, "node", "app/box.ts#Box.m299", 0, false);
    assert_eq!(
        method.value["node"]["parent"], "app/box.ts#Box",
        "{}",
        method.value
    );
    let (t, _) = super::super::format::decode(&original).unwrap();
    let id = |key: &str| {
        t.nodes
            .iter()
            .position(|node| t.str(node.key) == key)
            .unwrap()
    };
    let (file, parent, child) = (id("box.ts"), id("box.ts#Box"), id("box.ts#Box.m299"));
    // A sibling decoded only if rendered: in the parent's block, its key
    // id as wide as the parent's.
    let other = id("box.ts#Box.m010");
    assert!(
        file / 256 != parent / 256 && parent / 256 != child / 256 && other / 256 == parent / 256,
        "{file} {parent} {other} {child}"
    );
    // The parent's and the sibling's lookup keys swapped, and the key
    // index swapped with them so it stays in key order: the parent's entry
    // now names the sibling's key, and every index check still passes.
    reseal(&graph, &original, b"LOOK", |look| {
        let entries = lookup_entries(look);
        let key = |node: usize| {
            let at = entries[node].0 + 1;
            at..varint_end(look, at)
        };
        let (a, b) = (key(parent), key(other));
        assert_eq!(a.len(), b.len(), "equal key widths");
        let first = look[a.clone()].to_vec();
        look.copy_within(b.clone(), a.start);
        look[b].copy_from_slice(&first);
    });
    let crafted = std::fs::read(&graph).unwrap();
    reseal(&graph, &crafted, b"KEYX", |keyx| {
        let mut at = varint_end(keyx, 0);
        let mut slots = Vec::new();
        while at < keyx.len() {
            let end = varint_end(keyx, at);
            slots.push(at..end);
            at = end;
        }
        let find = |node: usize| {
            slots
                .iter()
                .find(|slot| keyx[(*slot).clone()] == varint(node)[..])
                .unwrap()
                .clone()
        };
        let (a, b) = (find(parent), find(other));
        assert_eq!(a.len(), b.len(), "equal id widths");
        let first = keyx[a.clone()].to_vec();
        keyx.copy_within(b.clone(), a.start);
        keyx[b].copy_from_slice(&first);
    });
    assert_misstated(&ask(&dir, "node", "app/box.ts#Box.m299", 0, false));
}
