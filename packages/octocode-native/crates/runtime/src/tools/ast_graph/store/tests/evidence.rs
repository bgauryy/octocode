use super::*;

#[test]
fn repeated_call_sites_survive_topology_and_paging() {
    let source =
        "function target() {}\nexport function run() { target(); target();\n target(); }\n";
    let dir = git_app(&[("calls.ts", source)]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let topology = ask(&dir.0, "callees", Some("calls.ts#run"), |_| {});
    assert_eq!(topology.value["total"], 1);
    let mut identities = std::collections::BTreeSet::new();
    let mut starts = std::collections::BTreeSet::new();
    for offset in 0..3 {
        let page = ask(&dir.0, "witnesses", Some("calls.ts#run"), |o| {
            o.edges = vec!["calls".into()];
            o.limit = Some(1);
            o.offset = Some(offset);
            o.to = Some("calls.ts#target".into());
        });
        assert_eq!(page.value["op"], "witnesses");
        assert_eq!(page.value["total"], 3, "{}", page.value);
        assert_eq!(page.value["results"].as_array().unwrap().len(), 1);
        let row = &page.value["results"][0];
        assert_eq!(row["file"], "calls.ts");
        assert_eq!(row["sourceDigest"].as_str().unwrap().len(), 64);
        assert_eq!(
            row["sourceDigest"],
            octocode_engine::digest::sha256(source.as_bytes())
        );
        assert_eq!(row["derivation"]["rule"], "calls:local");
        assert_eq!(row["derivation"]["version"], "graph-analysis:16/facts:13");
        let start = row["sourceRange"]["startByte"].as_u64().unwrap() as usize;
        let end = row["sourceRange"]["endByte"].as_u64().unwrap() as usize;
        assert_eq!(&source[start..end], "target()");
        assert!(starts.insert(start), "same-line call ranges collapsed");
        assert!(identities.insert(row["id"].as_str().unwrap().to_owned()));
        assert_eq!(page.value.get("next").is_some(), offset < 2);
    }
    let all = ask(&dir.0, "witnesses", Some("calls.ts"), |o| {
        o.edges = vec!["calls".into()];
    });
    assert_eq!(all.value["total"], 3);
}

#[test]
fn unresolved_sites_are_attributable_and_empty_population_is_unavailable() {
    let source = "def run():\n    missing()\n    other()\n";
    let dir = git_app(&[("unknown.py", source)]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.value["linkage"]["eligible"], 0);
    assert_eq!(built.value["linkage"]["excluded"], 2);
    assert!(built.value["linkage"]["ratio"].is_null());
    let first = ask(&dir.0, "unresolved", Some("unknown.py#run"), |o| {
        o.limit = Some(1);
    });
    assert_eq!(first.value["total"], 2);
    let second = ask(&dir.0, "unresolved", Some("unknown.py#run"), |o| {
        o.limit = Some(1);
        o.offset = Some(1);
    });
    assert_ne!(
        first.value["results"][0]["id"],
        second.value["results"][0]["id"]
    );
    assert_eq!(first.value["results"][0]["reason"], "not-declared-in-repo");
    assert_eq!(first.value["results"][0]["evidence"], "syntax");
    for out in [&first, &second] {
        let row = &out.value["results"][0];
        let start = row["sourceRange"]["startByte"].as_u64().unwrap() as usize;
        let end = row["sourceRange"]["endByte"].as_u64().unwrap() as usize;
        assert_eq!(
            &source[start..end],
            format!("{}()", row["targetName"].as_str().unwrap())
        );
        assert_eq!(
            row["sourceDigest"],
            octocode_engine::digest::sha256(source.as_bytes())
        );
        assert_eq!(
            row["derivation"]["rule"],
            "calls:unresolved:not-declared-in-repo"
        );
    }
}

#[test]
fn witness_confidence_filters_calls_without_hiding_noncall_evidence() {
    let dir = git_app(&[
        ("a.ts", "export function target() {}\n"),
        ("b.ts", "export function target() {}\n"),
        ("barrel.ts", "export * from './a'; export * from './b';\n"),
        ("main.ts", "import {target} from './barrel';\n"),
    ]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let out = ask(&dir.0, "witnesses", Some("main.ts"), |o| {
        o.edges = vec!["uses".into()];
        o.confidence = Some("high".into());
    });
    assert_eq!(out.value["total"], 2, "{}", out.value);
    for row in out.value["results"].as_array().unwrap() {
        assert_eq!(row["confidence"], "medium");
        assert!(
            row["sourceRange"].is_null(),
            "projection must not invent a byte range"
        );
        assert_eq!(row["derivation"]["rule"], "uses:reexport");
    }
}

#[test]
fn utf16_call_evidence_hashes_and_slices_the_decoded_parser_input() {
    let source =
        "const label = 'λ'; function known() {} export function run(){ known(); missing(); }\n";
    let dir = git_app(&[("encoded.ts", "")]);
    let bytes = [0xff, 0xfe]
        .into_iter()
        .chain(source.encode_utf16().flat_map(u16::to_le_bytes))
        .collect::<Vec<_>>();
    std::fs::write(dir.0.join("app/encoded.ts"), &bytes).unwrap();
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    for (operation, expression) in [("witnesses", "known()"), ("unresolved", "missing()")] {
        let out = ask(&dir.0, operation, Some("encoded.ts#run"), |o| {
            if operation == "witnesses" {
                o.edges = vec!["calls".into()];
            }
        });
        assert_eq!(out.value["total"], 1, "{}", out.value);
        let row = &out.value["results"][0];
        let start = row["sourceRange"]["startByte"].as_u64().unwrap() as usize;
        let end = row["sourceRange"]["endByte"].as_u64().unwrap() as usize;
        assert_eq!(&source[start..end], expression);
        assert_eq!(
            row["sourceDigest"],
            octocode_engine::digest::sha256(source.as_bytes())
        );
        assert_ne!(row["sourceDigest"], octocode_engine::digest::sha256(&bytes));
    }
}

#[test]
fn declaration_assessments_survive_snapshot_round_trip() {
    let dir = git_app(&[("clean.py", "def run():\n    return 1\n")]);
    ingest_root(&dir.0, &dir.0.join("app"), None, false);
    let out = ask(&dir.0, "node", Some("clean.py#run"), |_| {});
    assert_eq!(
        out.value["node"]["parseAssessment"]["status"], "parsed",
        "{}",
        out.value
    );
    assert!(
        out.value["node"]["sourceRange"]["endByte"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn mixed_languages_keep_call_capability_and_empty_populations_separate() {
    let dir = git_app(&[
        (
            "linked.ts",
            "function target() {} export function run() { target(); }\n",
        ),
        (
            "labels.asm",
            "target:\n mov body_marker, %rax\n call helper\n ret\n",
        ),
        ("quiet.py", "def quiet():\n    return 42\n"),
    ]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    assert_eq!(built.value["linkage"]["ratio"], 1.0);
    let supported = ask(&dir.0, "callees", Some("linked.ts"), |_| {});
    assert_eq!(supported.value["coverage"]["callExtraction"], "supported");
    assert_eq!(supported.value["coverage"]["linkage"]["ratio"], 1.0);
    for operation in ["callers", "callees"] {
        let unsupported = ask(&dir.0, operation, Some("labels.asm"), |_| {});
        let coverage = &unsupported.value["coverage"];
        assert_eq!(
            coverage["callExtraction"], "unsupported",
            "{}",
            unsupported.value
        );
        assert!(coverage["linkage"]["ratio"].is_null());
        assert_eq!(coverage["linkage"]["eligible"], 0);
        assert!(
            coverage["warning"]
                .as_str()
                .unwrap()
                .contains("do not establish absence")
        );
    }
    let quiet = ask(&dir.0, "callees", Some("quiet.py"), |_| {});
    assert_eq!(quiet.value["coverage"]["callExtraction"], "supported");
    assert_eq!(quiet.value["coverage"]["linkage"]["eligible"], 0);
    assert!(quiet.value["coverage"]["linkage"]["ratio"].is_null());

    let graph_dir = PathBuf::from(built.value["dir"].as_str().unwrap());
    let mut manifest = read_manifest(&graph_dir).unwrap();
    manifest
        .as_object_mut()
        .unwrap()
        .remove("callExtractionByLanguage");
    manifest["linkageByLanguage"]
        .as_object_mut()
        .unwrap()
        .remove("python");
    std::fs::write(
        graph_dir.join(MANIFEST_FILE),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let unavailable = ask(&dir.0, "callees", Some("quiet.py"), |_| {});
    assert_eq!(
        unavailable.value["coverage"]["callExtraction"],
        "notAssessed"
    );
    assert!(unavailable.value["coverage"]["linkage"]["ratio"].is_null());
}
