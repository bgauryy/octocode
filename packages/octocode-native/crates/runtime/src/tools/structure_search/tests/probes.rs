//! Empty-listing probes: the entry cap, every skip cause from one probe walk,
//! and the leads past name filters and exclusions.
use super::super::probe::TAGGED_PROBE_ENTRIES;
use super::*;

/// An empty listing's probe of what it skipped stops at its entry cap: it
/// proves "some" and names the reason without walking the whole directory.
#[test]
fn an_empty_listings_probe_stops_at_its_entry_cap() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("build/gen")).expect("build");
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");
    let cap = TAGGED_PROBE_ENTRIES;
    for index in 0..cap + 40 {
        std::fs::write(root.0.join(format!("build/gen/f{index:04}.ts")), "x\n").expect("ts");
    }
    for operation in ["files", "tree"] {
        let out = run(
            &root.0,
            json!({"operation": operation, "path": root.0, "extensions": ["ts"]}),
        )
        .expect("listing");
        assert_eq!(out["status"], "empty", "{out}");
        let hint = out["hints"][0].as_str().expect("hint");
        assert_eq!(
            hint,
            format!("At least {cap} entries behind skipped paths (build/)."),
            "{out}"
        );
        let retry = &out["next"]["includeSkipped"]["query"]["queries"][0];
        assert_eq!(retry["defaultExcludes"], false, "{out}");
        // The lead itself lists every one of them.
        let found = run(&root.0, retry.clone()).expect("retry");
        assert_ne!(found["status"], "empty", "{found}");
    }
}

/// An empty listing whose matches sit behind several skip causes (a dot
/// directory and a default-excluded one) walks once more to find why: one
/// walk with every cause lifted, its entries tagged with the causes that hid
/// them, answers every narrower relaxation the diagnosis asks about. The
/// lead lifts exactly the causes that hold matches, and the hint names both.
#[test]
fn an_empty_listing_diagnoses_every_skip_cause_from_one_probe_walk() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".notes")).expect("hidden dir");
    std::fs::create_dir_all(root.0.join("build")).expect("build");
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");
    std::fs::write(root.0.join(".notes/a.ts"), "x\n").expect("hidden ts");
    std::fs::write(root.0.join("build/b.ts"), "x\n").expect("built ts");
    for operation in ["files", "tree"] {
        let before = super::super::listing::listing_walks();
        let out = run(
            &root.0,
            json!({"operation": operation, "path": root.0, "extensions": ["ts"]}),
        )
        .expect("listing");
        let walks = super::super::listing::listing_walks() - before;
        assert_eq!(out["status"], "empty", "{out}");
        assert_eq!(walks, 2, "the listing and one probe walk: {out}");
        assert_eq!(
            out["hints"][0], "2 entries behind skipped paths (hidden .notes/, build/).",
            "{operation}: {out}"
        );
        let retry = &out["next"]["includeSkipped"]["query"]["queries"][0];
        assert_eq!(retry["includeHidden"], true, "{out}");
        assert_eq!(retry["defaultExcludes"], false, "{out}");
        let found = run(&root.0, retry.clone()).expect("retry");
        assert!(found.to_string().contains("a.ts"), "{found}");
        assert!(found.to_string().contains("b.ts"), "{found}");
    }
}

/// An empty listing names the ignore rule behind it workspace-relative,
/// like its rows, never relative to the listed directory.
#[test]
fn skip_reasons_name_ignore_files_workspace_relative() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "*.gen.ts\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("sub/inner")).expect("dirs");
    std::fs::write(root.0.join("sub/inner/a.gen.ts"), "x\n").expect("generated");
    for operation in ["files", "tree"] {
        let out = run(
            &root.0,
            json!({"operation": operation, "path": "sub/inner", "extensions": ["ts"]}),
        )
        .expect("listing");
        assert_eq!(out["status"], "empty", "{out}");
        assert_eq!(
            out["hints"][0], "1 entry behind skipped paths (.gitignore:1 '*.gen.ts').",
            "{operation}: {out}"
        );
    }
}

/// An entry the caller excluded is the caller's own filter: the walk drops
/// it before the path policy sees it, so it is never counted as withheld
/// or hidden.
#[test]
fn excluded_entries_are_not_counted_as_withheld_or_hidden() {
    let root = sensitive_fixture();
    for operation in ["tree", "files"] {
        let plain = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"includeHidden":true}),
        )
        .expect("listing");
        assert!(
            skipped_count(&plain, "withheld") > 0,
            "{operation}: {plain}"
        );
        let excluded = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"includeHidden":true,"exclude":["terraform.tfstate",".aws"]}),
        )
        .expect("excluded listing");
        let count = skipped_count(&excluded, "withheld");
        assert_eq!(count, 0, "{operation}: {excluded}");
        let hidden = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"exclude":[".aws"]}),
        )
        .expect("listing without hidden");
        assert_eq!(skipped_count(&hidden, "hidden"), 0, "{operation}: {hidden}");
    }
}

/// An empty listing its name filters alone emptied (nothing skipped to
/// lift) leads to the same listing without them (`broadenSearch`), which
/// lists what is there; a tree keeps the depth it walked.
#[test]
fn a_name_filter_miss_leads_to_the_listing_without_its_name_filters() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/deep")).expect("dir");
    std::fs::write(root.0.join("src/deep/lib.rs"), "x\n").expect("file");
    for query in [
        json!({"operation":"files","path":root.0,"nameRegex":"^nomatch$","entryType":"f"}),
        json!({"operation":"tree","path":root.0,"include":["*.nomatch"]}),
    ] {
        let out = run(&root.0, query.clone()).expect("empty listing");
        assert_eq!(out["status"], "empty", "{out}");
        let lead = &out["next"]["broadenSearch"];
        assert_eq!(lead["tool"], "structureSearch", "{out}");
        let row = &lead["query"]["queries"][0];
        for field in ["nameRegex", "include", "extensions"] {
            assert!(row.get(field).is_none(), "{field}: {row}");
        }
        assert_eq!(row["operation"], query["operation"], "{row}");
        assert!(
            out["hints"]
                .as_array()
                .is_some_and(|hints| hints.iter().any(|hint| hint
                    .as_str()
                    .is_some_and(|text| text.starts_with("No entry matches")))),
            "{out}"
        );
        let broadened = run(&root.0, row.clone()).expect("broadened listing");
        assert!(
            broadened.to_string().contains("lib.rs"),
            "{query}: {broadened}"
        );
    }
    // Nothing under the path at all: no lead to a listing that finds nothing.
    let empty = root.0.join("empty");
    std::fs::create_dir(&empty).expect("empty dir");
    let out = run(
        &root.0,
        json!({"operation":"files","path":empty,"nameRegex":"^nomatch$"}),
    )
    .expect("empty listing");
    assert!(out["next"].get("broadenSearch").is_none(), "{out}");
}
