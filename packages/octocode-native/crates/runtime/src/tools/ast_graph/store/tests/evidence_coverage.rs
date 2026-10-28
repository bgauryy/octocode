use super::*;

fn mixed_sources() -> (Fixture, GraphOutput) {
    let dir = git_app(&[
        (
            "linked.ts",
            "function target() {} export function run() { target(); target(); }\n",
        ),
        ("empty.ts", "export function empty() {}\n"),
        ("labels.asm", "target:\n call helper\n ret\n"),
        ("quiet.py", "def quiet():\n    return 42\n"),
    ]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    (dir, built)
}

fn evidence(dir: &Path, operation: &str, target: Option<&str>, to: Option<&str>) -> GraphOutput {
    ask(dir, operation, target, |options| {
        if operation == "witnesses" {
            options.edges = vec!["calls".into()];
            options.to = to.map(str::to_owned);
        }
    })
}

fn change_manifest(built: &GraphOutput, update: impl FnOnce(&mut serde_json::Value)) {
    let graph_dir = PathBuf::from(built.value["dir"].as_str().unwrap());
    let mut manifest = read_manifest(&graph_dir).unwrap();
    update(&mut manifest);
    std::fs::write(
        graph_dir.join(MANIFEST_FILE),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}

#[test]
fn evidence_source_scope_distinguishes_supported_empty_unsupported_and_unknown() {
    let (dir, built) = mixed_sources();
    for operation in ["witnesses", "unresolved"] {
        for (target, language, status) in [
            ("labels.asm", "asm", "unsupported"),
            ("quiet.py", "python", "supported"),
        ] {
            let out = evidence(&dir.0, operation, Some(target), None);
            assert_eq!(out.class, ExitClass::Empty, "{}", out.value);
            assert_eq!(out.value["total"], 0);
            let coverage = &out.value["coverage"];
            assert_eq!(coverage["language"], language);
            assert_eq!(coverage["callExtraction"], status);
            assert_eq!(coverage["linkage"]["eligible"], 0);
            assert!(coverage["linkage"]["ratio"].is_null());
            if status == "unsupported" {
                assert!(
                    coverage["warning"]
                        .as_str()
                        .unwrap()
                        .contains("do not establish absence")
                );
            }
        }
        let empty = evidence(&dir.0, operation, Some("empty.ts"), None);
        assert_eq!(empty.value["total"], 0);
        assert_eq!(empty.value["coverage"]["linkage"]["linked"], 2);
        assert!(
            empty.value["coverage"]["warning"]
                .as_str()
                .unwrap()
                .contains("whole-language population")
        );
    }

    change_manifest(&built, |manifest| {
        manifest["callExtractionByLanguage"]
            .as_object_mut()
            .unwrap()
            .remove("python");
        manifest["linkageByLanguage"]
            .as_object_mut()
            .unwrap()
            .remove("python");
    });
    for operation in ["witnesses", "unresolved"] {
        let unknown = evidence(&dir.0, operation, Some("quiet.py"), None);
        assert_eq!(unknown.value["total"], 0);
        assert_eq!(unknown.value["coverage"]["callExtraction"], "notAssessed");
        assert_eq!(unknown.value["coverage"]["linkage"]["eligible"], 0);
        assert!(unknown.value["coverage"]["linkage"]["ratio"].is_null());
        assert!(
            unknown.value["coverage"]["warning"]
                .as_str()
                .unwrap()
                .contains("do not establish absence")
        );
    }
}

#[test]
fn mixed_evidence_coverage_includes_sources_without_rows_and_ignores_destination_scope() {
    let (dir, built) = mixed_sources();
    for operation in ["witnesses", "unresolved"] {
        let out = evidence(&dir.0, operation, None, None);
        let coverage = &out.value["coverage"];
        assert_eq!(coverage["language"], "");
        assert_eq!(coverage["callExtraction"], "notAssessed");
        assert_eq!(coverage["linkage"], built.value["linkage"]);
        let warning = coverage["warning"].as_str().unwrap();
        assert!(warning.contains("unsupported=[asm]"));
        assert!(warning.contains("supported=[python, typescript]"));
        assert!(warning.contains("all source languages"));
        assert!(warning.contains("destination and evidence filters do not narrow"));
        if operation == "witnesses" {
            for destination in ["linked.ts#target", "labels.asm", "quiet.py"] {
                let filtered = evidence(&dir.0, operation, None, Some(destination));
                assert_eq!(filtered.value["coverage"], *coverage, "{}", filtered.value);
            }
        }
    }
    let source = evidence(&dir.0, "witnesses", Some("labels.asm"), None);
    let both = evidence(
        &dir.0,
        "witnesses",
        Some("labels.asm"),
        Some("linked.ts#target"),
    );
    assert_eq!(both.value["coverage"], source.value["coverage"]);
    assert_eq!(both.value["coverage"]["language"], "asm");

    change_manifest(&built, |manifest| {
        manifest["callExtractionByLanguage"]
            .as_object_mut()
            .unwrap()
            .remove("python");
    });
    for operation in ["witnesses", "unresolved"] {
        let out = evidence(&dir.0, operation, None, None);
        let warning = out.value["coverage"]["warning"].as_str().unwrap();
        assert!(warning.contains("unsupported=[asm]"));
        assert!(warning.contains("notAssessed=[python]"));
        assert_eq!(out.value["coverage"]["linkage"], built.value["linkage"]);
    }
}

#[test]
fn single_language_empty_evidence_preserves_recorded_capability() {
    for (path, source, language, status) in [
        (
            "quiet.py",
            "def quiet():\n    return 42\n",
            "python",
            "supported",
        ),
        ("labels.asm", "target:\n ret\n", "asm", "unsupported"),
    ] {
        let dir = git_app(&[(path, source)]);
        let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
        for operation in ["witnesses", "unresolved"] {
            let out = evidence(&dir.0, operation, None, None);
            assert_eq!(out.value["total"], 0);
            assert_eq!(out.value["coverage"]["language"], language);
            assert_eq!(out.value["coverage"]["callExtraction"], status);
        }
        change_manifest(&built, |manifest| {
            manifest
                .as_object_mut()
                .unwrap()
                .remove("callExtractionByLanguage");
        });
        for operation in ["witnesses", "unresolved"] {
            let out = evidence(&dir.0, operation, None, None);
            assert_eq!(out.value["coverage"]["callExtraction"], "notAssessed");
            assert_eq!(out.value["coverage"]["language"], language);
        }
    }
}

#[test]
fn aggregate_evidence_capability_uses_snapshot_values_and_survives_paging() {
    let (dir, built) = mixed_sources();
    for (recorded, expected) in [(true, "supported"), (false, "unsupported")] {
        change_manifest(&built, |manifest| {
            for capability in manifest["callExtractionByLanguage"]
                .as_object_mut()
                .unwrap()
                .values_mut()
            {
                *capability = serde_json::json!(recorded);
            }
        });
        for operation in ["witnesses", "unresolved"] {
            let out = evidence(&dir.0, operation, None, None);
            assert_eq!(out.value["coverage"]["callExtraction"], expected);
            assert_eq!(out.value["coverage"]["linkage"], built.value["linkage"]);
            if operation == "witnesses" {
                for offset in 0..2 {
                    let page = ask(&dir.0, operation, None, |options| {
                        options.edges = vec!["calls".into()];
                        options.limit = Some(1);
                        options.offset = Some(offset);
                    });
                    assert_eq!(page.value["coverage"], out.value["coverage"]);
                    assert_eq!(page.value["total"], 2);
                    assert_eq!(page.value["results"].as_array().unwrap().len(), 1);
                    assert_eq!(page.value.get("next").is_some(), offset == 0);
                }
            }
        }
    }
}
