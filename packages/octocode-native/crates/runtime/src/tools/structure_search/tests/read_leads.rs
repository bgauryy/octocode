//! The read lead of a listing: the top files outline, generated and large
//! files skipped, manifests first, ranked like ghStructure.

use super::*;

/// A `files` result's first page leads to its first outline-able file: a
/// localFetch `minify:"symbols"` read of its declarations. A listing of
/// only non-source files offers none, and later pages repeat none.
#[test]
fn a_files_result_leads_to_the_top_files_outline() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/blob.bin"), [0_u8, 1, 2]).expect("bin");
    std::fs::write(root.0.join("src/lib.rs"), "pub fn a() {}\n").expect("lib");
    std::fs::write(root.0.join("src/main.rs"), "fn main() {}\n").expect("main");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("src"),"include":["*.rs","*.bin"]}),
    )
    .expect("files");
    let lead = &out["next"]["read"];
    assert_eq!(lead["tool"], "localFetch", "{out}");
    let path = lead["query"]["queries"][0]["path"].as_str().expect("path");
    assert!(path.ends_with("src/lib.rs"), "{out}");
    assert!(std::path::Path::new(path).is_absolute(), "{out}");
    assert_eq!(lead["query"]["queries"][0]["minify"], "symbols", "{out}");
    assert_eq!(
        lead["query"]["queries"][0].as_object().map(|q| q.len()),
        Some(2),
        "{out}"
    );
    crate::contracts::validate_query("localFetch", lead["query"]["queries"][0].clone())
        .expect("read is a valid localFetch query");

    for name in ["c.rs", "d.rs", "e.rs"] {
        std::fs::write(root.0.join("src").join(name), "fn x() {}\n").expect("more");
    }
    let wide = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("src"),"include":["*.rs","*.bin"],"pageSize":2}),
    )
    .expect("files");
    assert!(wide.pointer("/next/read").is_some(), "{wide}");
    let later = run(
        &root.0,
        wide["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("page 2");
    assert!(later.pointer("/next/read").is_none(), "{later}");
    let binary = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("src"),"include":["*.bin"]}),
    )
    .expect("files");
    assert!(binary.pointer("/next/read").is_none(), "{binary}");
}

/// `tree` and `files` lead to the same read over the same rows: one
/// ranking ([`crate::tools::root_read::read_rank`]), filtered or not; an
/// outline-able file no tier names still leads when nothing better is
/// listed.
#[test]
fn tree_and_files_lead_to_the_same_read() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/util.rs"), "fn u() {}\n").expect("util");
    std::fs::write(root.0.join("src/main.rs"), "fn main() {}\n").expect("main");
    std::fs::create_dir(root.0.join("docs")).expect("docs");
    std::fs::write(root.0.join("docs/guide.md"), "# guide\n").expect("guide");
    let read = |query: Value| {
        let out = run(&root.0, query.clone()).expect("listing");
        out["next"]["read"]["query"]["queries"][0]["path"]
            .as_str()
            .map(|path| path.rsplit('/').next().unwrap_or(path).to_owned())
    };
    for extra in [json!({}), json!({"extensions":["rs"]})] {
        let mut leads = Vec::new();
        for operation in ["tree", "files"] {
            let mut query = json!({"operation":operation,"path":root.0.join("src")});
            query
                .as_object_mut()
                .expect("query")
                .extend(extra.as_object().expect("extra").clone());
            leads.push(read(query));
        }
        assert_eq!(leads, vec![Some("main.rs".to_owned()); 2], "{extra}");
    }
    for operation in ["tree", "files"] {
        assert_eq!(
            read(json!({"operation":operation,"path":root.0.join("docs")})),
            Some("guide.md".to_owned()),
            "{operation}"
        );
    }
}

/// The files read lead skips generated and large files.
#[test]
fn files_read_lead_skips_generated_and_large() {
    let root = Fixture::new();
    std::fs::write(
        root.0.join("a_types.generated.rs"),
        "pub struct A;\n".repeat(8_000),
    )
    .expect("generated");
    std::fs::write(root.0.join("b.rs"), "fn b() {}\n").expect("small");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("files");
    let read = out["next"]["read"]["query"]["queries"][0]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("read lead: {out}"));
    assert!(read.ends_with("b.rs"), "{out}");
}

/// A root tree leads to its manifest, read whole.
#[test]
fn root_tree_leads_to_manifest() {
    let root = Fixture::new();
    std::fs::write(root.0.join("package.json"), "{\"name\":\"x\"}\n").expect("manifest");
    std::fs::write(root.0.join("README.md"), "# x\n").expect("readme");
    std::fs::create_dir(root.0.join("src")).expect("src");
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("tree");
    let query = &out["next"]["read"]["query"]["queries"][0];
    assert!(
        query["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("package.json")),
        "{out}"
    );
    assert!(query.get("minify").is_none(), "{out}");
}

/// A tree ranks its read lead with the rule ghStructure uses
/// ([`crate::tools::root_read::read_rank`]): a code entry file, then the
/// manifest, README and guides, then loose source; a nested manifest or
/// README never leads; a bare-word-free root with only loose scripts and docs
/// reads the README before a script.
#[test]
fn tree_read_lead_ranks_like_ghstructure() {
    let lead = |files: &[&str]| {
        let root = Fixture::new();
        for file in files {
            let path = root.0.join(file);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(path, "# x\n").expect("file");
        }
        let out = run(
            &root.0,
            json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0}),
        )
        .expect("tree");
        let query = out["next"]["read"]["query"]["queries"][0].clone();
        let path = query["path"].as_str().expect("read lead").to_owned();
        (path, query.get("minify").cloned())
    };
    let leads = |files: &[&str], expected: &str| {
        let (path, _) = lead(files);
        assert!(path.ends_with(&format!("/{expected}")), "{files:?}: {path}");
    };
    leads(&["package.json", "index.ts"], "index.ts");
    leads(&["mm.mjs", "README.md"], "README.md");
    leads(&["mm.mjs", "AGENTS.md"], "AGENTS.md");
    let (path, minify) = lead(&["README.md", "package.json"]);
    assert!(path.ends_with("/package.json"), "{path}");
    assert_eq!(minify, None);
    // Docs outline like ghStructure's read of the same file.
    let (path, minify) = lead(&["README.md"]);
    assert!(path.ends_with("/README.md"), "{path}");
    assert_eq!(minify, Some(json!("symbols")));
    // Only the listed directory's own manifest describes it.
    leads(&["pkg/package.json", "util.rs"], "util.rs");
}
