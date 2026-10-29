//! Rust test code in production crates: `#[cfg(test)]` items and the
//! modules they mount are test code, and a module's own children are no
//! directory cycle.
use super::*;

/// A crate whose production files hold test code: an inline
/// `#[cfg(test)] mod tests` calling back into a module that imports this
/// one, a `#[cfg(test)] mod x;` mounting a directory of test files that use a
/// dev-dependency, a parent module and its child directory, and two sibling
/// directories that really do depend on each other.
fn crate_with_test_code() -> Fixture {
    git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dev-dependencies]\nwiremock = \"0.6\"\n",
        ),
        (
            "src/lib.rs",
            "pub mod a;\npub mod b;\npub mod contracts;\npub mod engine;\npub mod id;\npub mod tree;\n",
        ),
        (
            "src/contracts.rs",
            "use crate::id::Id;\npub fn parsed() -> Id { Id }\n",
        ),
        (
            "src/id.rs",
            "pub struct Id;\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn parses() {\n        let _ = crate::contracts::parsed();\n    }\n}\n",
        ),
        (
            "src/engine.rs",
            "pub fn run() -> u8 { 1 }\n\n#[cfg(test)]\nmod construction;\n",
        ),
        ("src/engine/construction.rs", "mod construct;\n"),
        (
            "src/engine/construction/construct.rs",
            "use wiremock::MockServer;\nuse crate::contracts::parsed;\n\n#[test]\nfn builds() {\n    let _ = parsed();\n    let _ = MockServer::builder;\n}\n",
        ),
        (
            "src/tree.rs",
            "pub mod child;\npub fn top() -> u8 { child::leaf() }\n",
        ),
        (
            "src/tree/child.rs",
            "use super::top;\npub fn leaf() -> u8 { 1 }\npub fn up() -> u8 { top() }\n",
        ),
        ("src/a.rs", "pub mod x;\n"),
        (
            "src/a/x.rs",
            "use crate::b::y::why;\npub fn ex() -> u8 { why() }\n",
        ),
        ("src/b.rs", "pub mod y;\n"),
        (
            "src/b/y.rs",
            "use crate::a::x::ex;\npub fn why() -> u8 { 0 }\npub fn back() -> u8 { ex() }\n",
        ),
    ])
}

fn detector(out: &GraphOutput, name: &str) -> Vec<Value> {
    out.value["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["detector"] == name)
        .cloned()
        .collect()
}

/// An edge from an inline `#[cfg(test)]` module is a test edge: no
/// production cycle, hub or issue counts it, and `deps` still lists it.
#[test]
fn cfg_test_module_edges_are_test_edges() {
    let dir = crate_with_test_code();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let cycles = ask(root, "cycles", None, |_| {});
    assert!(
        !cycles.value.to_string().contains("src/id.rs"),
        "a test-only call closes no cycle: {}",
        cycles.value
    );
    let issues = ask(root, "issues", None, |o| {
        o.limit = Some(200);
        o.min_tier = Some(0);
    });
    let cycle_members = detector(&issues, "cycle")
        .iter()
        .map(|row| row["evidence"]["members"].to_string())
        .collect::<Vec<_>>();
    assert!(
        cycle_members
            .iter()
            .all(|members| !members.contains("src/id.rs")),
        "{cycle_members:?}"
    );
    let hubs = ask(root, "hubs", None, |_| {});
    let contracts = hubs.value["results"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["id"] == "src/contracts.rs");
    assert!(
        contracts.is_none(),
        "only test code imports contracts.rs: {}",
        hubs.value
    );
    let deps = ask(root, "deps", Some("src/id.rs"), |_| {});
    let row = deps.value["results"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["id"] == "src/contracts.rs")
        .unwrap_or_else(|| panic!("the test edge stays queryable: {}", deps.value));
    assert_eq!(row["test"], true, "{row}");
    assert_eq!(row["edgeLine"], 9, "{row}");
}

/// A file a `#[cfg(test)] mod x;` mounts, and every module under it, is a
/// test file, so its dev-dependency import is no production finding.
#[test]
fn cfg_test_mounted_modules_are_test_files() {
    let dir = crate_with_test_code();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    for file in [
        "src/engine/construction.rs",
        "src/engine/construction/construct.rs",
    ] {
        let node = ask(root, "node", Some(file), |_| {});
        let roles = node.value["node"]["roles"].to_string();
        assert!(roles.contains("test"), "{file}: {}", node.value);
    }
    let engine = ask(root, "node", Some("src/engine.rs"), |_| {});
    assert!(
        !engine.value["node"]["roles"].to_string().contains("test"),
        "{}",
        engine.value
    );
    let issues = ask(root, "issues", None, |o| {
        o.limit = Some(200);
        o.min_tier = Some(0);
    });
    assert!(
        detector(&issues, "dev-dependency-in-production").is_empty(),
        "{}",
        issues.value
    );
}

/// A Rust module and its own child directory (`tree.rs` and `tree/`) are
/// one module tree, not a directory cycle; sibling directories that import
/// each other still are.
#[test]
fn rust_module_and_its_children_are_no_dir_cycle() {
    let dir = crate_with_test_code();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| {
        o.detectors = vec!["dir-cycle".into()];
        o.min_tier = Some(0);
    });
    let groups = detector(&issues, "dir-cycle")
        .iter()
        .map(|row| row["evidence"]["dirs"].clone())
        .collect::<Vec<_>>();
    assert_eq!(groups, [json!(["src/a", "src/b"])], "{}", issues.value);
}

/// Detectors that do not judge a language say so for the files they skip,
/// so an empty result is never a silent claim of absence.
#[test]
fn export_detectors_disclose_unjudged_languages() {
    let dir = crate_with_test_code();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| {
        o.detectors = vec!["unused-export".into(), "export-only-local".into()];
        o.min_tier = Some(0);
    });
    let notes = issues.value["summary"]["notes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for name in ["unused-export", "export-only-local"] {
        let note = notes
            .iter()
            .find(|note| note["detector"] == name)
            .unwrap_or_else(|| panic!("no {name} note: {}", issues.value));
        let text = note["skipped"].as_str().unwrap_or_default();
        assert!(text.contains("rust"), "{text}");
    }
}

/// A query with no snapshot names the workspace it looked in and how to
/// ingest or name another one.
#[test]
fn missing_graph_names_the_workspace_and_the_way_out() {
    let dir = Fixture::new();
    let root = dir.0.as_path();
    let out = ask(root, "stats", None, |_| {});
    let message = out.value["error"].as_str().unwrap_or_default().to_owned()
        + out.value["message"].as_str().unwrap_or_default();
    let workspace = root.display().to_string();
    assert!(message.contains(&workspace), "{}", out.value);
    assert!(message.contains("--workspace"), "{}", out.value);
    assert!(message.contains("octocode graph ingest"), "{}", out.value);
}

/// `use` edges within one Rust module tree (`tree.rs` and `tree/child.rs`)
/// are the tree, not coupling: the pair is no cycle, no hub and no misplaced
/// file. Sibling modules that import each other (`a/x.rs`, `b/y.rs`) still
/// form a cycle.
#[test]
fn rust_module_tree_edges_are_no_structural_coupling() {
    let dir = crate_with_test_code();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| {
        o.limit = Some(200);
        o.min_tier = Some(0);
    });
    let cycles = detector(&issues, "cycle")
        .iter()
        .map(|row| row["evidence"]["members"].to_string())
        .collect::<Vec<_>>();
    assert!(
        cycles.iter().all(|m| !m.contains("src/tree.rs")),
        "a module and its child are no cycle: {cycles:?}"
    );
    assert!(
        cycles
            .iter()
            .any(|m| m.contains("src/a/x.rs") && m.contains("src/b/y.rs")),
        "siblings importing each other still cycle: {cycles:?}"
    );
    for name in ["god-file", "misplaced-file", "single-point-of-failure"] {
        let subjects = detector(&issues, name)
            .iter()
            .map(|row| row["subject"].to_string())
            .collect::<Vec<_>>();
        assert!(
            subjects.iter().all(|s| !s.contains("src/tree.rs")),
            "{name}: {subjects:?}"
        );
    }
    let cycles = ask(root, "cycles", None, |_| {});
    assert!(
        !cycles.value.to_string().contains("src/tree.rs"),
        "`graph query cycles` agrees: {}",
        cycles.value
    );
}

/// A crate whose `hub.rs` is imported by five siblings and imports five
/// others: hub-like by fan-in and fan-out. With only `mod` declarations
/// and `pub use` lines of its own it is a module root (a barrel), so no
/// god-file; with declarations of its own it is one.
fn crate_with_hub(own_items: bool) -> Fixture {
    let mut files = vec![
        (
            "Cargo.toml".to_owned(),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n".to_owned(),
        ),
        (
            "src/lib.rs".to_owned(),
            "pub mod hub;\npub mod s1;\npub mod s2;\npub mod s3;\npub mod s4;\npub mod s5;\npub mod u1;\npub mod u2;\npub mod u3;\npub mod u4;\npub mod u5;\n".to_owned(),
        ),
        (
            "src/hub.rs".to_owned(),
            format!(
                "pub mod a;\npub mod b;\npub mod c;\npub use a::A;\nuse crate::u1::U1;\nuse crate::u2::U2;\nuse crate::u3::U3;\nuse crate::u4::U4;\nuse crate::u5::U5;\npub fn all() -> (U1, U2, U3, U4, U5) {{ (U1, U2, U3, U4, U5) }}\n{}",
                if own_items {
                    "pub struct Own1;\npub struct Own2;\n"
                } else {
                    ""
                }
            ),
        ),
        ("src/hub/a.rs".to_owned(), "pub struct A;\n".to_owned()),
        ("src/hub/b.rs".to_owned(), "pub struct B;\n".to_owned()),
        ("src/hub/c.rs".to_owned(), "pub struct C;\n".to_owned()),
    ];
    for n in 1..=5 {
        files.push((
            format!("src/s{n}.rs"),
            "use crate::hub::A;\npub fn f() -> A { A }\n".to_owned(),
        ));
        files.push((format!("src/u{n}.rs"), format!("pub struct U{n};\n")));
    }
    let borrowed = files
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect::<Vec<_>>();
    git_app(&borrowed)
}

#[test]
fn rust_module_roots_are_barrels_not_god_files() {
    for (own_items, expected) in [(false, 0), (true, 1)] {
        let dir = crate_with_hub(own_items);
        let root = dir.0.as_path();
        assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
        let issues = ask(root, "issues", None, |o| {
            o.detectors = vec!["god-file".into()];
            o.min_tier = Some(0);
        });
        let hubs = detector(&issues, "god-file")
            .iter()
            .filter(|row| row["subject"] == "src/hub.rs")
            .count();
        assert_eq!(hubs, expected, "own items {own_items}: {}", issues.value);
    }
}

/// A relative import that climbs above the snapshot root is reported apart
/// from a broken one: its target may exist beyond the scan.
#[test]
fn imports_past_the_snapshot_root_are_reported_as_outside() {
    let dir = git_app(&[
        (
            "src/a.ts",
            "import { x } from '../../../shared/x';\nexport const a = x;\n",
        ),
        (
            "src/b.ts",
            "import { y } from './missing';\nexport const b = y;\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| {
        o.detectors = vec!["unresolved-import".into()];
        o.min_tier = Some(0);
    });
    let rows = detector(&issues, "unresolved-import");
    let titled = |subject: &str| {
        rows.iter()
            .find(|row| row["subject"] == subject)
            .map(|row| {
                (
                    row["title"].as_str().unwrap_or_default().to_owned(),
                    row["evidence"]["outsideRoot"].as_bool().unwrap_or(false),
                )
            })
            .unwrap_or_else(|| panic!("no finding for {subject}: {}", issues.value))
    };
    let (title, outside) = titled("src/a.ts");
    assert!(
        outside && title.contains("outside the snapshot root"),
        "{title}"
    );
    let (title, outside) = titled("src/b.ts");
    assert!(
        !outside && title.contains("could not be resolved"),
        "{title}"
    );
}
