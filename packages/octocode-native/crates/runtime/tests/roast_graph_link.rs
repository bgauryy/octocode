//! Graph answers on a scope below its package root, read from the serialized
//! tool result: super chains that leave the scan link by directory, `crate::`
//! paths beyond the scan are scope gaps, a hidden tier has a runnable lead,
//! and every path is named from the one stated root.

use crate::support::{Workspace, call, row_data};
use serde_json::{Value, json};
use std::path::PathBuf;

/// A crate (`Cargo.toml`, `src/lib.rs`) whose `src/tools/ls` module is the
/// only scanned part; its parent module and `crate::id` are above the scan.
fn sub_scope(workspace: &Workspace) -> PathBuf {
    workspace.write(
        "Cargo.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    );
    workspace.write("src/lib.rs", "pub mod tools;\npub mod id;\n");
    workspace.write("src/id.rs", "pub struct X;\n");
    workspace.write("src/tools/mod.rs", "pub mod ls;\n");
    workspace.write("src/tools/ls.rs", "pub mod executor;\npub mod layout;\n");
    workspace.write(
        "src/tools/ls/executor.rs",
        "mod projection;\nmod settle;\npub fn run() {}\n",
    );
    workspace.write(
        "src/tools/ls/executor/projection.rs",
        "use super::super::layout::Layout;\nuse crate::id::X;\npub fn p(_: Layout, _: X) {}\n",
    );
    workspace.write(
        "src/tools/ls/executor/settle.rs",
        "use super::super::layout::Layout;\npub fn s(_: Layout) {}\n",
    );
    workspace.write("src/tools/ls/layout.rs", "pub struct Layout;\n");
    workspace.workspace.join("src/tools/ls")
}

async fn build(runtime: &octocode_native::runtime::ToolRuntime, path: &PathBuf) -> Value {
    let built = call(runtime, "graphBuild", json!({"path": path}))
        .await
        .expect("graphBuild");
    row_data(&built).clone()
}

async fn query(runtime: &octocode_native::runtime::ToolRuntime, input: Value) -> Value {
    let out = call(runtime, "graphQuery", input)
        .await
        .expect("graphQuery");
    row_data(&out).clone()
}

fn ids(data: &Value) -> Vec<String> {
    data["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect()
}

fn related_builds(data: &Value) -> Vec<&Value> {
    data["related"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|lead| lead["tool"] == "graphBuild")
        .collect()
}

#[tokio::test]
async fn impact_on_a_sub_scope_lists_the_super_super_dependents() {
    let workspace = Workspace::new();
    let scan = sub_scope(&workspace);
    let runtime = workspace.runtime(&[]);
    build(&runtime, &scan).await;
    let data = query(
        &runtime,
        json!({"operation":"impact","path":scan,"target":"layout.rs"}),
    )
    .await;
    let listed = ids(&data);
    for dependent in ["executor/projection.rs", "executor/settle.rs"] {
        assert!(
            listed.iter().any(|id| id.ends_with(dependent)),
            "{dependent} missing: {data}"
        );
    }
    let diagnostics = query(
        &runtime,
        json!({"operation":"diagnostics","path":scan,"target":"unresolved-internal"}),
    )
    .await;
    assert!(
        !diagnostics.to_string().contains("super::super"),
        "{diagnostics}"
    );
    assert!(
        !related_builds(&data).is_empty(),
        "the widen-scope lead is missing: {data}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_build_below_the_package_root_counts_crate_paths_as_outside_and_leads_to_the_root() {
    let workspace = Workspace::new();
    let scan = sub_scope(&workspace);
    let runtime = workspace.runtime(&[]);
    let receipt = build(&runtime, &scan).await;
    let diagnostics = &receipt["diagnostics"];
    assert!(
        diagnostics["unresolved-internal"].is_null()
            || diagnostics["unresolved-internal"] == json!(0),
        "{receipt}"
    );
    assert!(
        diagnostics["outside-root"].as_u64().unwrap_or(0) >= 1,
        "{receipt}"
    );
    let first = &receipt["related"][0];
    assert_eq!(first["tool"], "graphBuild", "{receipt}");
    runtime.close().await;
}

/// A scan of `src/local_search`, whose own module file `local_search.rs`
/// lies above it: `cursor.rs` declares a test module, and both
/// `local_search/tests.rs` and `cursor/tests.rs` exist.
fn orphan_scope(workspace: &Workspace) -> PathBuf {
    workspace.write(
        "Cargo.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    );
    workspace.write("src/lib.rs", "pub mod local_search;\n");
    workspace.write(
        "src/local_search.rs",
        "pub mod cursor;\n#[cfg(test)]\nmod tests;\n",
    );
    workspace.write(
        "src/local_search/cursor.rs",
        "pub fn cut() {}\n#[cfg(test)]\nmod tests;\n",
    );
    workspace.write(
        "src/local_search/cursor/tests.rs",
        "use super::cut;\n#[test]\nfn cuts() { cut(); }\n",
    );
    workspace.write("src/local_search/tests.rs", "#[test]\nfn top() {}\n");
    workspace.workspace.join("src/local_search")
}

#[tokio::test]
async fn a_test_module_beside_an_orphan_root_links_to_its_own_file() {
    let workspace = Workspace::new();
    let scan = orphan_scope(&workspace);
    let runtime = workspace.runtime(&[]);
    build(&runtime, &scan).await;
    let diagnostics = query(
        &runtime,
        json!({"operation":"diagnostics","path":scan,"target":"unresolved-internal"}),
    )
    .await;
    assert!(
        !diagnostics.to_string().contains("self::tests"),
        "{diagnostics}"
    );
    let issues = query(
        &runtime,
        json!({"operation":"issues","path":scan,"detectors":["unresolved-import"],"minTier":60}),
    )
    .await;
    assert!(!issues.to_string().contains("cursor.rs"), "{issues}");
    runtime.close().await;
}

/// `index.ts` loads `a1..a6`, each importing `hub.ts`; `orphan.ts` is loaded by nothing.
fn hub_scope(workspace: &Workspace) -> PathBuf {
    workspace.write("package.json", "{\"name\":\"app\",\"main\":\"index.ts\"}");
    let mut index = String::new();
    for n in 1..=6 {
        index.push_str(&format!("import './a{n}';\n"));
        workspace.write(
            &format!("a{n}.ts"),
            "import { h } from './hub';\nexport const x = h;\n",
        );
    }
    workspace.write("index.ts", index);
    workspace.write("hub.ts", "export const h = 1;\n");
    workspace.write("orphan.ts", "export const o = 1;\n");
    workspace.workspace.clone()
}

fn detectors_of(data: &Value) -> Vec<String> {
    data["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["detector"].as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn informational_detectors_run_only_when_named_and_sort_after_actionable_rows() {
    let workspace = Workspace::new();
    let scan = hub_scope(&workspace);
    let runtime = workspace.runtime(&[]);
    build(&runtime, &scan).await;
    let default = query(&runtime, json!({"operation":"issues","path":scan})).await;
    assert!(
        !default.to_string().contains("critical-file"),
        "an unnamed informational detector ran: {default}"
    );
    let named = query(
        &runtime,
        json!({"operation":"issues","path":scan,"detectors":["critical-file","unreachable-file"]}),
    )
    .await;
    let order = detectors_of(&named);
    let first_info = order
        .iter()
        .position(|d| d == "critical-file")
        .expect("critical-file listed");
    let last_actionable = order
        .iter()
        .rposition(|d| d == "unreachable-file")
        .expect("unreachable listed");
    assert!(last_actionable < first_info, "{order:?}");
    runtime.close().await;
}

#[tokio::test]
async fn a_hidden_tier_has_a_runnable_lead() {
    let workspace = Workspace::new();
    let scan = sub_scope(&workspace);
    let runtime = workspace.runtime(&[]);
    build(&runtime, &scan).await;
    let issues = query(
        &runtime,
        json!({"operation":"issues","path":scan,"detectors":["unresolved-import"]}),
    )
    .await;
    assert!(
        issues["rows"].as_array().is_none_or(Vec::is_empty),
        "{issues}"
    );
    let lead = issues["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|lead| lead["tool"] == "graphQuery")
        .unwrap_or_else(|| panic!("no hidden-tier lead: {issues}"));
    let input = &lead["input"]["queries"][0];
    assert_eq!(input["minTier"], 60, "{lead}");
    let shown = call(&runtime, "graphQuery", lead["input"].clone())
        .await
        .expect("lead runs");
    assert!(
        !ids_of_findings(row_data(&shown)).is_empty(),
        "the lead lists the hidden rows: {}",
        row_data(&shown)
    );
    runtime.close().await;
}

fn ids_of_findings(data: &Value) -> Vec<String> {
    data["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect()
}

/// A directory the walk may not enter is named even when no scanned file
/// sits in its parent.
#[cfg(unix)]
#[tokio::test]
async fn a_nested_unreadable_directory_is_named_under_a_parent_with_no_scanned_file() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = Workspace::new();
    workspace.write("top.ts", "export const top = 1;\n");
    workspace.write("outer/inner/hidden.ts", "export const hidden = 1;\n");
    let inner = workspace.workspace.join("outer/inner");
    std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let enforced = std::fs::read_dir(&inner).is_err();
    let runtime = workspace.runtime(&[]);
    let receipt = build(&runtime, &workspace.workspace).await;
    std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o755)).expect("restore");
    if !enforced {
        // Privileges that ignore the mode: nothing to name.
        runtime.close().await;
        return;
    }
    assert!(
        receipt.to_string().contains("outer/inner"),
        "the unreadable directory is not named: {receipt}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn hubs_rows_leave_out_what_the_id_and_the_description_state() {
    let workspace = Workspace::new();
    let scan = hub_scope(&workspace);
    let runtime = workspace.runtime(&[]);
    build(&runtime, &scan).await;
    let hubs = query(&runtime, json!({"operation":"hubs","path":scan})).await;
    let rows = hubs["rows"].as_array().expect("hub rows");
    assert!(!rows.is_empty(), "{hubs}");
    for row in rows {
        assert!(row.get("language").is_none(), "{row}");
        assert!(row.get("kind").is_none(), "an id tells its kind: {row}");
    }
    // Whether the counts hold test edges is the ranking's fact, stated once.
    assert_eq!(hubs["testEdges"], "excluded", "{hubs}");
    runtime.close().await;
}

#[tokio::test]
async fn impact_states_every_file_reference_from_one_base() {
    let workspace = Workspace::new();
    let scan = sub_scope(&workspace);
    let runtime = workspace.runtime(&[]);
    build(&runtime, &scan).await;
    let data = query(
        &runtime,
        json!({"operation":"impact","path":scan,"target":"layout.rs"}),
    )
    .await;
    // The changed file is its own depth-0 row (summary.changed does not
    // repeat it), and every `from` names a row from the same base.
    let rows = data["rows"].as_array().expect("impact rows");
    assert_eq!(rows[0]["risk"], "changed", "{data}");
    assert!(data["summary"].get("changed").is_none(), "{data}");
    let ids = rows.iter().map(|row| &row["id"]).collect::<Vec<_>>();
    for row in rows.iter().filter(|row| row.get("from").is_some()) {
        assert!(ids.contains(&&row["from"]), "{row}: {data}");
    }
    runtime.close().await;
}

/// A call target that only spells a placeholder was never replaced: the flag
/// follows what the build did, not the text a row shows.
#[tokio::test]
async fn a_source_that_spells_a_placeholder_is_not_flagged_as_replaced() {
    let workspace = Workspace::new();
    workspace.write(
        "computed.ts",
        "const client: any = {};\nexport function callIt(): void {\n  client[\"[REDACTED-GITHUB-TOKEN]\"]();\n}\n",
    );
    let runtime = workspace.runtime(&[]);
    build(&runtime, &workspace.workspace).await;
    let rows = query(
        &runtime,
        json!({"operation":"unresolved","path":workspace.workspace,"target":"computed.ts"}),
    )
    .await;
    assert!(
        rows.to_string().contains("[REDACTED-GITHUB-TOKEN]"),
        "the literal target shows: {rows}"
    );
    assert_ne!(rows["redacted"], json!(true), "{rows}");
    runtime.close().await;
}

#[tokio::test]
async fn graph_rows_inside_the_workspace_name_files_from_it_and_calls_run_as_given() {
    let workspace = Workspace::new();
    let scan = sub_scope(&workspace);
    let runtime = workspace.runtime(&[]);
    build(&runtime, &scan).await;
    let data = query(
        &runtime,
        json!({"operation":"impact","path":scan,"target":"layout.rs"}),
    )
    .await;
    // The workspace is the base: no other root is stated.
    assert!(data["graph"].get("root").is_none(), "{data}");
    let listed = ids(&data);
    assert!(!listed.is_empty(), "{data}");
    for id in listed.iter().filter(|id| !id.contains('#')) {
        assert!(workspace.workspace.join(id).is_file(), "{id}: {data}");
    }
    // Every call runs as given.
    let leads = data["related"].as_array().cloned().unwrap_or_default();
    assert!(!leads.is_empty(), "{data}");
    for lead in leads {
        let ran = call(
            &runtime,
            lead["tool"].as_str().expect("tool"),
            lead["input"].clone(),
        )
        .await
        .expect("the lead runs as given");
        assert!(
            row_data(&ran).get("errorCode").is_none(),
            "{lead}: {}",
            row_data(&ran)
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_graph_outside_the_workspace_names_rows_from_its_absolute_root() {
    let workspace = Workspace::new();
    let outside = Workspace::new();
    outside.write(
        "package.json",
        "{\"name\":\"ext\",\"main\":\"src/index.ts\"}",
    );
    outside.write(
        "src/index.ts",
        "import { helper } from './util/helper';\nexport const main = () => helper();\n",
    );
    outside.write("src/util/helper.ts", "export const helper = () => 1;\n");
    let runtime = workspace.runtime(&[
        (
            "ALLOWED_PATHS",
            format!(
                "{},{}",
                workspace.workspace.display(),
                outside.workspace.display()
            ),
        ),
        (
            "OCTOCODE_GRAPH_HOME",
            workspace.home.join("graphs").to_string_lossy().into_owned(),
        ),
    ]);
    let receipt = build(&runtime, &outside.workspace).await;
    assert_eq!(receipt["root"], json!(outside.workspace), "{receipt}");
    let data = query(
        &runtime,
        json!({"operation":"dependents","path":outside.workspace,"target":"src/util/helper.ts"}),
    )
    .await;
    let root = std::path::Path::new(data["graph"]["root"].as_str().expect("root"));
    assert_eq!(root, outside.workspace, "{data}");
    let listed = ids(&data);
    assert_eq!(listed, ["src/index.ts"], "{data}");
    assert!(root.join(&listed[0]).is_file(), "{data}");
    runtime.close().await;
}

/// A scan of `src/tools/ls` whose own module file `src/tools/ls.rs` lies
/// above it: its items, reached by `crate::` paths, `use`, a visibility
/// path and a `super` climb to the scan's module, are code above the scan.
fn parent_module_scope(workspace: &Workspace) -> PathBuf {
    let scan = sub_scope(workspace);
    workspace.write(
        "src/tools/ls.rs",
        "pub mod executor;\npub mod layout;\npub mod reader;\npub struct Status;\npub fn listing() {}\n",
    );
    workspace.write(
        "src/tools/ls/reader.rs",
        "use crate::tools::ls::Status;\nuse super::*;\npub(in crate::tools::ls) fn helper() {}\npub fn read() -> Status {\n    crate::tools::ls::listing();\n    let _ = crate::tools::ls::executor::run;\n    super::listing();\n    Status\n}\n",
    );
    scan
}

#[tokio::test]
async fn paths_into_the_scans_own_module_file_are_outside_the_root() {
    let workspace = Workspace::new();
    let scan = parent_module_scope(&workspace);
    let runtime = workspace.runtime(&[]);
    let receipt = build(&runtime, &scan).await;
    let internal = query(
        &runtime,
        json!({"operation":"diagnostics","path":scan,"target":"unresolved-internal"}),
    )
    .await;
    assert!(
        internal["rows"].as_array().is_none_or(Vec::is_empty),
        "an item of the scan's module file reads as a broken link: {internal}"
    );
    let outside = query(
        &runtime,
        json!({"operation":"diagnostics","path":scan,"target":"outside-root"}),
    )
    .await;
    let named = outside.to_string();
    for spec in ["crate::tools::ls::Status", "super::*"] {
        assert!(
            named.contains(spec),
            "{spec} is not outside-root: {outside}"
        );
    }
    // `pub(in crate::tools::ls)` names where an item is visible, not a use
    // of that module: no reference, so no diagnostic either.
    assert!(!named.contains("\\\"crate::tools::ls\\\""), "{outside}");
    // A path into a module the scan holds links.
    assert!(!named.contains("executor::run"), "{outside}");
    assert!(
        receipt["diagnostics"].get("unresolved-internal").is_none(),
        "{receipt}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_package_root_at_the_workspace_root_is_named_as_such() {
    let workspace = Workspace::new();
    let scan = sub_scope(&workspace);
    let runtime = workspace.runtime(&[]);
    let receipt = build(&runtime, &scan).await;
    let warning = receipt["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find(|text| text.contains("package root"))
        .unwrap_or_else(|| panic!("no package-root warning: {receipt}"));
    assert!(warning.contains("the workspace root"), "{warning}");
    assert!(
        !warning.contains(" ..") && !warning.contains("root ."),
        "{warning}"
    );
    let lead = related_builds(&receipt)[0];
    let built = call(&runtime, "graphBuild", lead["input"].clone())
        .await
        .expect("the lead runs as given");
    assert_eq!(
        row_data(&built)["root"],
        ".",
        "it builds the workspace root: {lead}"
    );
    runtime.close().await;
}

/// An app whose entry imports code beyond the scan (a tier-60 finding,
/// hidden by default) beside an unreferenced file (a listed finding).
fn hidden_beside_listed(workspace: &Workspace) -> PathBuf {
    workspace.write(
        "app/package.json",
        "{\"name\":\"app\",\"main\":\"index.ts\"}",
    );
    workspace.write(
        "app/index.ts",
        "import { shared } from '../shared/util';\nexport const run = () => shared();\n",
    );
    workspace.write("app/orphan.ts", "export const o = 1;\n");
    workspace.write("shared/util.ts", "export const shared = () => 1;\n");
    workspace.workspace.join("app")
}

#[tokio::test]
async fn a_page_that_lists_findings_still_names_and_carries_the_hidden_tier_call() {
    let workspace = Workspace::new();
    let scan = hidden_beside_listed(&workspace);
    let runtime = workspace.runtime(&[]);
    build(&runtime, &scan).await;
    let issues = query(&runtime, json!({"operation":"issues","path":scan})).await;
    assert!(
        issues["rows"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "{issues}"
    );
    let hidden = issues["summary"]["hiddenBelowTier"].as_u64().unwrap_or(0);
    assert!(hidden > 0, "{issues}");
    let lead = issues["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|lead| {
            lead["tool"] == "graphQuery" && lead["input"]["queries"][0]["minTier"].is_u64()
        })
        .unwrap_or_else(|| panic!("no hidden-tier call: {issues}"));
    let tier = lead["input"]["queries"][0]["minTier"]
        .as_u64()
        .expect("tier");
    // The note names the call by what tells it apart, not by its place.
    assert!(
        issues["warnings"]
            .to_string()
            .contains(&format!("graphQuery call with minTier {tier}")),
        "{issues}"
    );
    let shown = call(&runtime, "graphQuery", lead["input"].clone())
        .await
        .expect("lead runs");
    assert_eq!(
        row_data(&shown)["summary"]["hiddenBelowTier"],
        json!(0),
        "{}",
        row_data(&shown)
    );
    runtime.close().await;
}

/// A call row of a file a read hides a value of states its range in the
/// text a read shows: two calls on one line stay two places, a call past
/// the hidden value moves by its placeholder, and none shows the value.
#[tokio::test]
async fn rows_of_a_file_with_a_hidden_value_keep_their_shown_ranges() {
    let workspace = Workspace::new();
    let token = format!("ghp_{}", "a1B2c3D4e5".repeat(4).get(..36).expect("36"));
    workspace.write(
        "a.js",
        format!(
            "export function editor(values, input) {{\n  values.forEach(item => {{ const o = String(item); input.value = String(item); }});\n  const token = \"{token}\";\n  input.title = String(token);\n}}\n"
        ),
    );
    let runtime = workspace.runtime(&[]);
    build(&runtime, &workspace.workspace).await;
    let rows = query(
        &runtime,
        json!({"operation":"unresolved","path":workspace.workspace,"target":"a.js"}),
    )
    .await;
    assert!(!rows.to_string().contains(&token), "{rows}");
    let fetched = call(
        &runtime,
        "localFetch",
        json!({"path": workspace.workspace.join("a.js")}),
    )
    .await
    .expect("localFetch");
    let shown = row_data(&fetched)["content"]
        .as_str()
        .expect("content")
        .lines()
        .map(|line| line.split_once('\t').map_or(line, |(_, text)| text))
        .collect::<Vec<_>>()
        .join("\n");
    let calls = rows["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["targetName"] == "String")
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 3, "{rows}");
    let mut ranges = Vec::new();
    for row in &calls {
        let range = &row["sourceRange"];
        let (start, end) = (
            range["startByte"]
                .as_u64()
                .unwrap_or_else(|| panic!("no range: {row}")) as usize,
            range["endByte"].as_u64().expect("end") as usize,
        );
        let text = shown.get(start..end).unwrap_or_else(|| panic!("{row}"));
        assert!(text.starts_with("String("), "{text:?} at {row}");
        ranges.push((start, end));
    }
    ranges.dedup();
    assert_eq!(
        ranges.len(),
        3,
        "two calls on one line stay two places: {rows}"
    );
    runtime.close().await;
}
