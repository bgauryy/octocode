//! Rust rows: feature-gated mentions and negated gates.
use super::*;

#[test]
fn rust_rows_flag_feature_gated_mentions_and_offer_an_all_features_rerun() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("lib.rs"), "pub fn spawn_blocking() {}\n").expect("lib");
    std::fs::write(
        dir.path().join("gated.rs"),
        "#![cfg(feature = \"full\")]\nfn t() { spawn_blocking(); }\n",
    )
    .expect("gated");
    std::fs::write(
        dir.path().join("other.rs"),
        "#[cfg(feature = \"x\")]\nfn spawn_blocking_extra() {}\n",
    )
    .expect("other");
    let root = dir.path().to_string_lossy().into_owned();
    let scope = Scope::new(root.clone(), vec!["*.rs".into()]);
    let row_query = json!({"path":format!("{root}/lib.rs"),"operation":"references","symbolName":"spawn_blocking","line":1});
    let mut row = json!({"kind":"references","matches":[{"path":"lib.rs"}]});
    annotate(
        &mut row,
        &query(row_query.clone()),
        Some("rust"),
        Server {
            anchor: &format!("{root}/lib.rs"),
            root: &root,
        },
        &scope,
    );
    // Gated files found: the rerun page and one reason, no prose.
    assert_eq!(row["partialReasons"], json!([CFG_REASON]), "{row}");
    assert!(
        row.get("coverage")
            .is_none_or(|c| c.get("reason").is_none()),
        "{row}"
    );
    assert!(row.get("warnings").is_none(), "{row}");
    let rerun = &row["next"]["expandFeatures"]["query"]["queries"][0];
    assert_eq!(rerun["rustContext"]["features"], "all", "{row}");
    assert_eq!(rerun["symbolName"], "spawn_blocking", "{row}");
    // A later page of the same walk keeps the reason; the rerun lead
    // was offered once, on the first page.
    for later in [
        json!({"page": 2, "snapshot": "lsp-v3:abc"}),
        json!({"importerPage": 2, "snapshot": "lsp-v3:abc"}),
    ] {
        let mut later_query = row_query.clone();
        later_query
            .as_object_mut()
            .expect("query")
            .extend(later.as_object().expect("fields").clone());
        let mut page = json!({"kind":"references","matches":[{"path":"lib.rs"}]});
        annotate(
            &mut page,
            &query(later_query),
            Some("rust"),
            Server {
                anchor: &format!("{root}/lib.rs"),
                root: &root,
            },
            &scope,
        );
        assert_eq!(page["partialReasons"], json!([CFG_REASON]), "{page}");
        assert!(page.pointer("/next/expandFeatures").is_none(), "{page}");
    }
    // An all-features run is not flagged again.
    let mut all = json!({"kind":"references","matches":[]});
    let mut all_query = row_query;
    all_query["rustContext"] = json!({"features":"all"});
    annotate(
        &mut all,
        &query(all_query),
        Some("rust"),
        Server {
            anchor: &format!("{root}/lib.rs"),
            root: &root,
        },
        &scope,
    );
    assert!(all.get("isPartial").is_none(), "{all}");
}

#[test]
fn all_features_run_flags_negated_feature_gates() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("lib.rs"), "pub fn spawn_blocking() {}\n").expect("lib");
    std::fs::write(
        dir.path().join("fallback.rs"),
        "#[cfg(not(feature = \"rt\"))]\nfn t() { spawn_blocking(); }\n",
    )
    .expect("negated");
    let root = dir.path().to_string_lossy().into_owned();
    let scope = Scope::new(root.clone(), vec!["*.rs".into()]);
    let mut row = json!({"kind":"references","matches":[]});
    let q = query(
        json!({"path":format!("{root}/lib.rs"),"operation":"references",
        "symbolName":"spawn_blocking","line":1,"rustContext":{"features":"all"}}),
    );
    annotate(
        &mut row,
        &q,
        Some("rust"),
        Server {
            anchor: &format!("{root}/lib.rs"),
            root: &root,
        },
        &scope,
    );
    assert_eq!(row["partialReasons"], json!([CFG_NEGATED_REASON]), "{row}");
    assert_eq!(row["next"]["findText"]["tool"], "localSearch", "{row}");
    assert!(row["next"].get("expandFeatures").is_none(), "{row}");
    assert!(has_negated_feature_gate(
        "#[cfg(all(unix, not( feature = \"x\")))]"
    ));
    assert!(!has_negated_feature_gate("#[cfg(feature = \"x\")]"));
}
