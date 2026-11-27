//! Guidance by row type: a listing that returns entries states no hint and
//! offers no related call, whatever it skipped; an empty listing states at
//! most one hint and offers at most one related call.
use super::*;

/// A work tree with a source folder, an ignored folder, a hidden folder and
/// an empty one.
fn workspace() -> Fixture {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::create_dir(root.0.join("empty")).expect("empty dir");
    root.write(".gitignore", "generated/\n");
    root.write("generated/out.ts", "x\n");
    root.write(".cache/kept.ts", "x\n");
    for n in 0..6 {
        root.write(&format!("src/mod{n}.rs"), "x\n");
    }
    root.write("docs/readme.md", "x\n");
    root
}

#[test]
fn a_listing_with_entries_states_no_hint_and_offers_no_related_call() {
    let root = workspace();
    let mut skipped = 0;
    for query in [
        json!({"operation": "tree", "path": root.0}),
        json!({"operation": "tree", "path": root.0, "maxDepth": 3}),
        json!({"operation": "files", "path": root.0}),
        json!({"operation": "files", "path": root.0, "view": "details"}),
        json!({"operation": "files", "path": root.0, "extensions": ["rs"]}),
        json!({"operation": "files", "path": "src", "nameRegex": "^mod[0-3]"}),
    ] {
        let out = run(&root.0, query.clone()).expect("listing");
        assert!(out.get("status").is_none(), "{query}: {out}");
        assert_listing_guides_nothing(&out);
        skipped += usize::from(out.get("skippedPaths").is_some());
    }
    assert!(skipped > 0, "a listing with entries beside skipped paths");
    // Every page of a paged listing: its continuation lists on, and nothing
    // else is offered.
    for operation in ["tree", "files"] {
        let pages = walk_pages_budget(
            &root.0,
            json!({"operation": operation, "path": root.0, "maxDepth": 3}),
            Some(1),
        );
        assert!(pages.len() > 1, "{operation}: the fixture pages");
        for page in pages.iter().filter(|page| !listed(page).is_empty()) {
            assert_listing_guides_nothing(page);
        }
    }
}

#[test]
fn an_empty_listing_states_one_hint_and_offers_one_related_call_at_most() {
    let root = workspace();
    // Skipped paths may hold the entry: the one call lifts their causes, and
    // lists it.
    for operation in ["tree", "files"] {
        let out = run(
            &root.0,
            json!({"operation": operation, "path": root.0, "extensions": ["ts"], "maxDepth": 3}),
        )
        .expect("listing");
        let call = skipped_recovery(&out);
        assert_eq!(call["includeIgnored"], true, "{operation}: {out}");
        let found = run(&root.0, call).expect("the offered listing");
        assert!(found["files"].to_string().contains("out.ts"), "{found}");
    }
    // A name filter that matches nothing, with nothing skipped: the one call
    // lists without it.
    let out = run(
        &root.0,
        json!({"operation": "files", "path": "src", "nameRegex": "^absent"}),
    )
    .expect("listing");
    let call = one_recovery(&out).unwrap_or_else(|| panic!("a call: {out}"));
    assert!(call.get("nameRegex").is_none(), "{out}");
    assert_eq!(out["hints"].as_array().map(Vec::len), Some(1), "{out}");
    let found = run(&root.0, call).expect("the offered listing");
    assert_listing_guides_nothing(&found);
    // An empty folder: nothing explains it, so nothing is stated or offered.
    for operation in ["tree", "files"] {
        let out = run(&root.0, json!({"operation": operation, "path": "empty"})).expect("listing");
        assert_eq!(one_recovery(&out), None, "{operation}: {out}");
        assert!(out.get("hints").is_none(), "{operation}: {out}");
    }
}
