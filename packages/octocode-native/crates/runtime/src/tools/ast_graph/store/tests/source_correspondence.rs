use super::*;
use crate::response::source_evidence;

fn assert_closure(value: &Value) {
    let units =
        source_evidence::referenced_units(value, &mut Default::default(), &NeverCancel).unwrap();
    if units.is_empty() {
        return;
    }
    let evidence = &value["sourceEvidence"];
    let rows = evidence["units"].as_array().expect("complete unit closure");
    assert_eq!(
        rows.len(),
        units.len(),
        "closure must contain exactly emitted units: {value}"
    );
    let mut representation_ids = BTreeSet::new();
    for unit in rows {
        assert!(units.contains(unit["id"].as_str().unwrap()));
        representation_ids.insert(unit["representationId"].as_str().unwrap());
    }
    let representations = evidence["representations"].as_array().unwrap();
    assert_eq!(representations.len(), representation_ids.len());
    let mut raw_ids = BTreeSet::new();
    for representation in representations {
        assert!(representation_ids.contains(representation["id"].as_str().unwrap()));
        for id in representation["sourceIds"].as_array().unwrap() {
            raw_ids.insert(id.as_str().unwrap());
        }
    }
    let snapshots = evidence["snapshots"].as_array().unwrap();
    assert_eq!(snapshots.len(), raw_ids.len());
    for source in snapshots {
        assert!(raw_ids.contains(source["id"].as_str().unwrap()));
    }
}

#[test]
fn saved_raw_identity_is_immutable_after_rewrites_and_encoding_changes() {
    let source = "const label = 'λ🙂'; function known() {} export function run(){ known(); }\n";
    let dir = git_app(&[("encoded.ts", "")]);
    let path = dir.0.join("app/encoded.ts");
    let raw = [0xff, 0xfe]
        .into_iter()
        .chain(source.encode_utf16().flat_map(u16::to_le_bytes))
        .collect::<Vec<_>>();
    std::fs::write(&path, &raw).unwrap();
    let first = ingest_root(&dir.0, &dir.0.join("app"), Some(10), false);
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    let first_id = first.value["id"].as_str().unwrap().to_owned();
    let original = ask(&dir.0, "node", Some("encoded.ts#run"), |o| {
        o.graph = Some(first_id.clone());
    });
    assert_eq!(original.class, ExitClass::Success, "{}", original.value);
    assert_closure(&original.value);
    assert_eq!(
        original.value["sourceEvidence"]["snapshots"][0]["encoding"],
        "utf16le"
    );
    assert_eq!(
        original.value["sourceEvidence"]["snapshots"][0]["digest"],
        octocode_engine::digest::sha256(&raw)
    );
    assert_eq!(
        original.value["node"]["sourceDigest"],
        octocode_engine::digest::sha256(source.as_bytes())
    );

    // Force a new graph while the original file reuses its neutral extraction cache.
    dir.write("app/other.ts", "export function other() {}\n");
    let warm = ingest_root(&dir.0, &dir.0.join("app"), Some(10), false);
    assert_eq!(warm.class, ExitClass::Success, "{}", warm.value);
    assert!(warm.value["extraction"]["reusedFiles"].as_u64().unwrap() >= 1);
    let warm_node = ask(&dir.0, "node", Some("encoded.ts#run"), |_| {});
    assert_eq!(
        warm_node.value["node"]["sourceOrigin"],
        original.value["node"]["sourceOrigin"]
    );
    assert_eq!(
        warm_node.value["sourceEvidence"],
        original.value["sourceEvidence"]
    );

    // An encoding-only rewrite has the same parser input and different captured origins.
    std::fs::write(&path, source).unwrap();
    let rewritten = ingest_root(&dir.0, &dir.0.join("app"), Some(10), false);
    assert_eq!(rewritten.class, ExitClass::Success, "{}", rewritten.value);
    let current = ask(&dir.0, "node", Some("encoded.ts#run"), |_| {});
    assert_eq!(
        current.value["node"]["sourceDigest"],
        original.value["node"]["sourceDigest"]
    );
    assert_ne!(
        current.value["node"]["sourceOrigin"]["analysisUnitId"],
        original.value["node"]["sourceOrigin"]["analysisUnitId"]
    );
    assert_eq!(
        current.value["sourceEvidence"]["snapshots"][0]["encoding"],
        "utf8"
    );
    std::fs::remove_file(&path).unwrap();
    let saved = ask(&dir.0, "node", Some("encoded.ts#run"), |o| {
        o.graph = Some(first_id);
    });
    assert_eq!(saved.class, ExitClass::Success, "{}", saved.value);
    assert_eq!(saved.value["node"], original.value["node"]);
    assert_eq!(
        saved.value["sourceEvidence"],
        original.value["sourceEvidence"]
    );
}

#[test]
fn every_query_identity_and_argument_page_carries_exact_descriptor_closure() {
    let source = "function handler() {} function sink(value: unknown) {} export function run(){ sink(handler); sink(handler); }\n";
    let dir = git_app(&[
        ("main.ts", source),
        ("other.ts", "export function handler() {}\n"),
    ]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    for (op, target) in [
        ("find", "handler"),
        ("node", "handler"),
        ("symbols", "main.ts"),
        ("callers", "main.ts#sink"),
        ("callees", "main.ts#run"),
        ("stats", ""),
    ] {
        let result = ask(&dir.0, op, (!target.is_empty()).then_some(target), |_| {});
        assert_closure(&result.value);
        assert!(
            !source_evidence::referenced_units(
                &result.value,
                &mut Default::default(),
                &NeverCancel
            )
            .unwrap()
            .is_empty(),
            "{op}: {}",
            result.value
        );
    }
    let path = ask(&dir.0, "path", Some("main.ts#run"), |o| {
        o.to = Some("main.ts#sink".into());
        o.edges = vec!["calls".into()];
    });
    assert_eq!(path.class, ExitClass::Success, "{}", path.value);
    assert_closure(&path.value);
    for offset in 0..2 {
        let page = ask(&dir.0, "witnesses", Some("main.ts#run"), |o| {
            o.edges = vec!["passes".into()];
            o.limit = Some(1);
            o.offset = Some(offset);
        });
        assert_eq!(page.class, ExitClass::Success, "{}", page.value);
        assert_eq!(page.value["total"], 2);
        let row = &page.value["results"][0];
        for origin in [
            &row["sourceOrigin"],
            &row["selectionOrigin"],
            &row["recipient"]["sourceOrigin"],
        ] {
            assert!(origin["analysisUnitId"].is_string(), "{}", page.value);
        }
        assert_closure(&page.value);
    }
}

#[test]
fn registry_only_toml_provider_reaches_normal_ingest_store_find_and_stats() {
    use octocode_engine::signatures::languages::{RegistryView, conformance::toml_registry};
    use octocode_engine::text::analysis::AnalysisOperation;
    let registry = toml_registry();
    let provider = registry.find("toml").unwrap();
    assert!(provider.admits(AnalysisOperation::GraphFacts));
    assert!(provider.admits(AnalysisOperation::Declarations));
    for operation in [
        AnalysisOperation::StructuralSearch,
        AnalysisOperation::SyntaxTree,
        AnalysisOperation::SignatureOutline,
    ] {
        assert!(!provider.admits(operation));
    }
    assert!(RegistryView::installed().find("toml").is_none());
    let dir = git_app(&[("settings.cfg", "\"設定\" = 1\nfeature = true\n")]);
    let options = IngestOptions {
        path: dir.0.join("app"),
        workspace: Some(dir.0.to_path_buf()),
        language_globs: BTreeMap::from([("toml".into(), vec!["*.cfg".into()])]),
        ..Default::default()
    };
    let built = super::super::ingest::ingest_with_registry(
        &options,
        &workspace_policy(&dir.0),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        registry,
        &crate::tools::ast_graph::limits::LIMITS,
    );
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let found = ask(&dir.0, "find", Some("設定"), |_| {});
    assert_eq!(found.class, ExitClass::Success, "{}", found.value);
    assert_eq!(found.value["total"], 1);
    assert_eq!(found.value["results"][0]["name"], "設定");
    assert_closure(&found.value);
    assert_eq!(
        found.value["sourceEvidence"]["units"][0]["adapter"]["id"],
        "fixture-toml-keys"
    );
    let stats = ask(&dir.0, "stats", None, |_| {});
    assert_eq!(stats.class, ExitClass::Success, "{}", stats.value);
    assert_eq!(stats.value["counts"]["nodesByKind"]["symbol"], 2);
    let stale = ask(&dir.0, "stale", None, |_| {});
    assert_eq!(stale.class, ExitClass::Success, "{}", stale.value);
    assert_eq!(stale.value["total"], 0, "{}", stale.value);
    let reused = super::super::ingest::ingest_with_registry(
        &options,
        &workspace_policy(&dir.0),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        registry,
        &crate::tools::ast_graph::limits::LIMITS,
    );
    assert_eq!(reused.class, ExitClass::Success, "{}", reused.value);
    assert_eq!(reused.value["id"], built.value["id"]);
    assert!(RegistryView::installed().find("toml").is_none());
}

#[test]
fn nested_region_declarations_selections_and_blockers_keep_their_units() {
    let source = "fn outside() {}\nimpl Host { items! { fn generated(callback: usize) { sink(callback); sink(outside); } } }\nfn healthy(local: usize) { sink(local); }\n";
    let dir = git_app(&[("macro.rs", source)]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let symbols = ask(&dir.0, "symbols", Some("macro.rs"), |_| {});
    assert_eq!(symbols.class, ExitClass::Success, "{}", symbols.value);
    assert_closure(&symbols.value);
    let rows = symbols.value["results"].as_array().unwrap();
    let generated = rows.iter().find(|row| row["name"] == "generated").unwrap();
    let node = ask(&dir.0, "node", generated["id"].as_str(), |_| {});
    let parent = rows
        .iter()
        .find(|row| row["id"] == node.value["node"]["parent"])
        .unwrap();
    let root_unit = &parent["sourceOrigin"]["analysisUnitId"];
    let region_unit = &generated["sourceOrigin"]["analysisUnitId"];
    assert_ne!(root_unit, region_unit);
    assert_eq!(&generated["selectionOrigin"]["analysisUnitId"], region_unit);
    assert_eq!(node.value["node"]["parent"], parent["id"]);
    assert_closure(&node.value);
    let references = ask(&dir.0, "unresolved", Some("macro.rs"), |o| {
        o.edges = vec!["passes".into()];
    });
    assert_eq!(references.class, ExitClass::Success, "{}", references.value);
    assert_closure(&references.value);
    let rows = references.value["results"].as_array().unwrap();
    let callback = rows
        .iter()
        .find(|row| row["targetName"] == "callback")
        .unwrap();
    for field in ["sourceOrigin", "selectionOrigin", "bindingOrigin"] {
        assert_eq!(
            &callback[field]["analysisUnitId"], region_unit,
            "{field}: {}",
            references.value
        );
    }
    assert_eq!(
        &callback["recipient"]["sourceOrigin"]["analysisUnitId"],
        region_unit
    );
    let local = rows
        .iter()
        .find(|row| row["targetName"] == "local")
        .unwrap();
    assert_eq!(
        &local["bindingOrigin"]["analysisUnitId"], root_unit,
        "{}",
        references.value
    );
}

#[test]
fn registry_identity_prevents_reuse_and_missing_inventory_is_an_explicit_error() {
    let dir = git_app(&[("plain.ts", "export function stable() {}\n")]);
    let first = ingest_root(&dir.0, &dir.0.join("app"), Some(10), false);
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    let graph_dir = PathBuf::from(first.value["dir"].as_str().unwrap());
    let mut manifest = read_manifest(&graph_dir).unwrap();
    manifest["registryIdentity"] = json!("0".repeat(64));
    std::fs::write(
        graph_dir.join(MANIFEST_FILE),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let rebuilt = ingest_root(&dir.0, &dir.0.join("app"), Some(10), false);
    assert_eq!(rebuilt.class, ExitClass::Success, "{}", rebuilt.value);
    assert_ne!(rebuilt.value["id"], first.value["id"]);
    manifest
        .as_object_mut()
        .unwrap()
        .remove("registryExtensions");
    std::fs::write(
        graph_dir.join(MANIFEST_FILE),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let invalid = ask(&dir.0, "node", Some("plain.ts#stable"), |o| {
        o.graph = first.value["id"].as_str().map(str::to_owned);
    });
    assert_eq!(invalid.value["errorCode"], "decode");
    assert!(
        invalid.value["error"]
            .as_str()
            .unwrap()
            .contains("registry inventory")
    );
}
