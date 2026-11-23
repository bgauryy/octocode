//! The opt-in SCIP tier of a build: consent, spawns, cache, fallback and the
//! build identity. The indexer is a stand-in script; no real index is run.
use super::*;
use octocode_engine::lsp::config::LspDiscoveryOptions;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::test_support::{Fixture, workspace_policy};

/// A rust-analyzer stand-in: `--version` answers; `scip` records the run,
/// fails when a `fail` file exists, else copies `index.bin` to `--output`.
const FAKE: &str = r#"#!/bin/sh
d=$(dirname "$0")
case "$1" in
--version) echo "rust-analyzer 1.0.0-fake"; exit 0;;
scip)
  echo run >> "$d/runs"
  [ -f "$d/fail" ] && exit 3
  out=""
  while [ $# -gt 0 ]; do [ "$1" = "--output" ] && out="$2"; shift; done
  cp "$d/index.bin" "$out"
  exit 0;;
esac
exit 2
"#;

/// An empty SCIP `Index` carrying only an empty `Metadata`.
const EMPTY_INDEX: &[u8] = &[0x0a, 0x00];

struct Project {
    dir: Fixture,
    discovery: LspDiscoveryOptions,
}

impl Project {
    fn new(consent: bool) -> Self {
        let dir = Fixture::new();
        dir.write(
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        );
        dir.write("src/lib.rs", "pub fn a() {}\npub fn b() { a() }\n");
        let tools = dir.0.join("tools");
        std::fs::create_dir_all(&tools).expect("tools");
        octocode_io::atomic::write_atomic_mode(
            &tools.join("rust-analyzer"),
            FAKE.as_bytes(),
            0o700,
            false,
        )
        .expect("fake server");
        std::fs::write(tools.join("index.bin"), EMPTY_INDEX).expect("index");
        let home = dir.0.join("home");
        std::fs::create_dir_all(&home).expect("home");
        let enabled = serde_json::json!({"enable": consent});
        let config = serde_json::json!({"languageServers": {".rs": {
            "command": tools.join("rust-analyzer"), "languageId": "rust",
            "initializationOptions": {"cargo": {"buildScripts": enabled}, "procMacro": enabled},
        }}});
        std::fs::write(home.join("lsp-servers.json"), config.to_string()).expect("config");
        let discovery = LspDiscoveryOptions {
            octocode_home: Some(home),
            env: [("PATH".to_owned(), "/usr/bin:/bin".to_owned())].into(),
            ..LspDiscoveryOptions::default()
        };
        Self { dir, discovery }
    }

    fn build(&self, refresh: bool) -> GraphOutput {
        let options = IngestOptions {
            path: self.dir.0.clone(),
            workspace: Some(self.dir.0.clone()),
            refresh,
            lsp: Some(self.discovery.clone()),
            ..IngestOptions::default()
        };
        ingest(
            &options,
            &workspace_policy(&self.dir.0),
            &ContentSecurity::new(),
            &NeverCancel,
            None,
        )
    }

    fn runs(&self) -> usize {
        std::fs::read_to_string(self.dir.0.join("tools/runs"))
            .map_or(0, |text| text.lines().count())
    }
}

#[test]
fn without_consent_a_rust_build_spawns_nothing() {
    let project = Project::new(false);
    let out = project.build(false);
    assert!(out.value.get("error").is_none(), "{}", out.value);
    assert_eq!(out.scip, ScipStatus::Off);
    assert_eq!(project.runs(), 0, "no consent, no indexer");
}

#[test]
fn a_build_with_no_discovery_settings_spawns_nothing() {
    let project = Project::new(true);
    let options = IngestOptions {
        path: project.dir.0.clone(),
        workspace: Some(project.dir.0.clone()),
        lsp: None,
        ..IngestOptions::default()
    };
    let out = ingest(
        &options,
        &workspace_policy(&project.dir.0),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(out.scip, ScipStatus::Off);
    assert_eq!(project.runs(), 0);
}

#[test]
fn with_consent_the_indexer_runs_once_and_later_builds_use_the_cache() {
    let project = Project::new(true);
    let first = project.build(true);
    assert!(first.value.get("error").is_none(), "{}", first.value);
    assert_eq!(first.scip, ScipStatus::Indexed);
    assert_eq!(project.runs(), 1);
    // A forced rebuild of unchanged inputs reads the cached index.
    let second = project.build(true);
    assert_eq!(second.scip, ScipStatus::Indexed);
    assert_eq!(project.runs(), 1, "the cached index serves the rebuild");
    // A build of an unchanged tree answers with its snapshot.
    let third = project.build(false);
    assert_eq!(third.scip, ScipStatus::Indexed);
    assert_eq!(project.runs(), 1);
}

#[test]
fn a_failing_indexer_leaves_a_successful_syntax_only_build_that_retries() {
    let project = Project::new(true);
    std::fs::write(project.dir.0.join("tools/fail"), "").expect("fail");
    let out = project.build(false);
    assert!(out.value.get("error").is_none(), "{}", out.value);
    assert_eq!(out.scip, ScipStatus::Failed);
    assert_eq!(project.runs(), 1);
    // The failed snapshot is not "current": the next build runs it again.
    std::fs::remove_file(project.dir.0.join("tools/fail")).expect("heal");
    let healed = project.build(false);
    assert_eq!(healed.scip, ScipStatus::Indexed);
    assert_eq!(project.runs(), 2);
}

#[test]
fn an_unreadable_index_falls_back_to_syntax_only_edges() {
    let project = Project::new(true);
    std::fs::write(project.dir.0.join("tools/index.bin"), [0xff, 0xff, 0xff]).expect("garbage");
    let out = project.build(false);
    assert!(out.value.get("error").is_none(), "{}", out.value);
    assert_eq!(out.scip, ScipStatus::Failed);
}

#[test]
fn toggling_consent_never_reuses_the_other_snapshot() {
    let off = Project::new(false);
    let first = off.build(false);
    assert_eq!(
        off.build(false).value["graph"],
        first.value["graph"],
        "reused when unchanged"
    );
    // The same tree and settings, but consent on.
    let enabled = Project::new(true);
    let plain = {
        let options = IngestOptions {
            path: enabled.dir.0.clone(),
            workspace: Some(enabled.dir.0.clone()),
            lsp: None,
            ..IngestOptions::default()
        };
        ingest(
            &options,
            &workspace_policy(&enabled.dir.0),
            &ContentSecurity::new(),
            &NeverCancel,
            None,
        )
    };
    let indexed = enabled.build(false);
    assert_ne!(
        plain.value["graph"], indexed.value["graph"],
        "{}",
        indexed.value
    );
    assert_eq!(indexed.scip, ScipStatus::Indexed);
}

#[test]
fn a_member_crate_build_indexes_the_workspace_and_reads_it_by_scope_paths() {
    let project = Project::new(true);
    project
        .dir
        .write("Cargo.toml", "[workspace]\nmembers = [\"crates/a\"]\n");
    project.dir.write(
        "crates/a/Cargo.toml",
        "[package]\nname = \"a\"\nversion = \"0.1.0\"\n",
    );
    project.dir.write("crates/a/src/lib.rs", "pub fn a() {}\n");
    std::fs::write(
        project.dir.0.join("tools/index.bin"),
        scip_tier::rebase_tests::index_of(&["crates/a/src/lib.rs"]),
    )
    .expect("index");
    let options = IngestOptions {
        path: project.dir.0.join("crates/a"),
        workspace: Some(project.dir.0.clone()),
        lsp: Some(project.discovery.clone()),
        ..IngestOptions::default()
    };
    let out = ingest(
        &options,
        &workspace_policy(&project.dir.0),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert!(out.value.get("error").is_none(), "{}", out.value);
    assert_eq!(out.scip, ScipStatus::Indexed);
}

/// A call syntax cannot bind (the receiver's type is declared nowhere), the
/// definitions of both candidate methods, and the index's reference at the
/// call naming `B::hidden`.
const UNBOUND: &str = "pub struct A;\npub struct B;\nimpl A { pub fn hidden(&self) {} }\nimpl B { pub fn hidden(&self) {} }\npub fn go(x: Unknown) {\n    x.hidden();\n}\n";

fn unbound_index() -> Vec<u8> {
    use crate::ast_graph::store::scip::tests::{UTF8, at, document, index, occ, occurrence, sym};
    let definition = |nth: usize, symbol: &str| {
        let (line, col) = at(UNBOUND, nth, "fn hidden");
        occurrence(&[line, col[0] + 3, col[0] + 9], &sym(symbol), 1)
    };
    let occurrences = [
        definition(0, "A#hidden"),
        definition(1, "B#hidden"),
        occ(UNBOUND, 2, "hidden", &sym("B#hidden"), 0, UTF8),
    ];
    index(&[document("src/lib.rs", UTF8, &occurrences)])
}

#[test]
fn an_indexed_build_states_its_tier_and_answers_the_call_from_the_index() {
    let project = Project::new(true);
    project.dir.write("src/lib.rs", UNBOUND);
    std::fs::write(project.dir.0.join("tools/index.bin"), unbound_index()).expect("index");
    let out = project.build(false);
    assert!(out.value.get("error").is_none(), "{}", out.value);
    assert_eq!(out.value["scip"], "indexed", "{}", out.value);
    let options = crate::ast_graph::store::QueryOptions {
        op: "callers".into(),
        target: Some("src/lib.rs#B.hidden".into()),
        workspace: Some(project.dir.0.clone()),
        ..Default::default()
    };
    let callers =
        crate::ast_graph::store::query::query(&options, &workspace_policy(&project.dir.0));
    let text = callers.value.to_string();
    assert!(
        text.contains("src/lib.rs#go"),
        "the index names the target: {text}"
    );
    assert!(
        text.contains("\"scip\""),
        "its witness says where it came from: {text}"
    );
    assert!(
        text.contains("syntax+scip"),
        "coverage states the basis: {text}"
    );
    // The same scope again is reused, and says the same.
    let again = project.build(false);
    assert_eq!(again.value["reused"], true, "{}", again.value);
    assert_eq!(again.value["scip"], "indexed", "{}", again.value);
}

#[test]
fn a_failed_tier_is_stated_with_its_reason_and_an_off_tier_not_at_all() {
    let project = Project::new(true);
    std::fs::write(project.dir.0.join("tools/fail"), "").expect("fail");
    let out = project.build(false);
    assert_eq!(out.value["scip"], "failed", "{}", out.value);
    let warnings = out.value["warnings"].to_string();
    assert!(warnings.contains("SCIP index failed"), "{}", out.value);
    let off = Project::new(false).build(false);
    assert!(off.value.get("scip").is_none(), "{}", off.value);
}
