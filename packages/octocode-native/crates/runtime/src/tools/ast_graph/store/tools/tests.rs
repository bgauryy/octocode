use super::*;
use crate::tools::test_support::{Fixture, workspace_policy};

fn context() -> ExecutionContext {
    ExecutionContext {
        cancellation: tokio_util::sync::CancellationToken::new(),
        deadline: Instant::now() + std::time::Duration::from_secs(60),
        walk_threads: None,
        response_window: None,
        github_credential: None,
    }
}

fn call(dir: &Path, tool: ToolId, row: Value) -> Value {
    let row = crate::contracts::validate_built_query(tool.as_str(), row).expect("input contract");
    let result = execute(
        tool,
        &row,
        &workspace_policy(dir),
        &ContentSecurity::new(),
        &context(),
        None,
    )
    .expect("execute");
    crate::tools::test_support::assert_row_contract(tool, &result.data);
    result.data
}

#[test]
fn argument_evidence_variants_and_continuations_match_the_generated_contract() {
    let dir = Fixture::new();
    dir.write("app/args.ts", "function handler(){} export function run(){ sink(handler, handler); sink(...items); } function shadow(handler:unknown){ sink(handler); }");
    dir.write("app/macros.rs", "fn main(){ consume!(handler); }");
    dir.write(
        "app/dynamic.js",
        "function handler(){} function run(){ eval(source); consume(handler); }",
    );
    dir.write(
        "app/recovered.js",
        "function handler(){} function broken(){ const missing; consume(handler); }",
    );
    let built = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    assert_eq!(
        built["argumentCoverage"]["population"],
        "syntax-argument-slots"
    );
    let mut reasons = BTreeSet::new();
    for operation in ["witnesses", "unresolved"] {
        let mut input = json!({"operation":operation,"path":"app","edges":["passes"],"limit":1});
        let mut ids = BTreeSet::new();
        loop {
            let page = call(&dir.0, ToolId::GraphQuery, input);
            assert_eq!(page["argumentCoverage"], built["argumentCoverage"]);
            for row in page["results"].as_array().expect("results") {
                assert_eq!(row["edge"], "passes");
                assert!(ids.insert(row["id"].as_str().expect("id").to_owned()));
                if let Some(reason) = row["reason"].as_str() {
                    reasons.insert(reason.to_owned());
                }
            }
            match page.pointer("/next/page/input/queries/0") {
                Some(next) => {
                    assert_eq!(next["edges"], json!(["passes"]));
                    input = next.clone();
                }
                None => {
                    assert_eq!(ids.len() as u64, page["total"]);
                    break;
                }
            }
        }
        assert!(!ids.is_empty());
    }
    for expected in [
        "binding-unrepresented",
        "unsupported-argument-form",
        "unsupported-argument-group",
        "scope-not-assessed",
        "recovered-occurrence",
    ] {
        assert!(
            reasons.contains(expected),
            "missing {expected}: {reasons:?}"
        );
    }
}

#[test]
fn build_reuse_pages_and_freshness_are_workspace_scoped() {
    let dir = Fixture::new();
    dir.write(
        "app/a.ts",
        "export function first() { return 1; }\nexport function final() { return first(); }\n",
    );
    dir.write("other/a.ts", "export const other = 2;\n");
    let built = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    assert!(
        Path::new(built["dir"].as_str().expect("dir")).starts_with(
            std::fs::canonicalize(&dir.0)
                .expect("canonical root")
                .join(".octocode/graph")
        )
    );
    let reused = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    assert_eq!(built["id"], reused["id"]);
    assert_eq!(reused["reused"], true);
    let stats = call(
        &dir.0,
        ToolId::GraphQuery,
        built["related"][0]["input"]["queries"][0].clone(),
    );
    assert_eq!(stats["graph"]["id"], built["id"]);
    let callers = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"callers", "path":"app", "target":"a.ts#first"}),
    );
    assert_eq!(
        callers["confidence"], "high",
        "the core contract supplies the default"
    );
    assert_eq!(callers["results"][0]["id"], "a.ts#final");
    call(&dir.0, ToolId::GraphBuild, json!({"path":"other"}));
    let mut row = json!({"operation":"find", "path":"app", "target":"f", "limit":1});
    let mut keys = BTreeSet::new();
    loop {
        let page = call(&dir.0, ToolId::GraphQuery, row);
        assert_eq!(page["graph"]["id"], built["id"]);
        assert_eq!(page["freshness"]["fresh"], true);
        for item in page["results"].as_array().expect("results") {
            assert!(keys.insert(item.to_string()), "duplicate row");
        }
        match page.pointer("/next/page/input/queries/0") {
            Some(next) => row = next.clone(),
            None => {
                assert_eq!(keys.len() as u64, page["total"]);
                break;
            }
        }
    }
    assert!(keys.len() >= 2);
    dir.write("app/a.ts", "export function first() { return 3; }\n");
    let stale = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"stats", "path":"app", "graph":built["id"]}),
    );
    assert_eq!(stale["freshness"]["fresh"], false);
    let wrong = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"stats", "path":"other", "graph":built["id"]}),
    );
    assert_eq!(wrong["errorCode"], "invalidInput");
}

#[test]
fn missing_snapshots_offer_build_recovery_for_the_requested_scope() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    dir.write("other/a.ts", "export const other = 2;\n");
    let built = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    for row in [
        json!({"operation":"stats", "path":"other"}),
        json!({"operation":"stats", "path":"app", "graph":"missing-snapshot"}),
        json!({"operation":"drift", "path":"app", "graph":built["id"], "baseline":"missing-baseline"}),
    ] {
        let mut output = call(&dir.0, ToolId::GraphQuery, row.clone());
        crate::response::rows::attach_error_hint(ToolId::GraphQuery, &mut output, &row);
        assert_eq!(output["errorCode"], "notFound", "{output}");
        let hints = output["hints"].as_array().expect("hints");
        assert!(
            hints
                .iter()
                .any(|hint| hint.as_str().is_some_and(|s| s.contains("graphBuild"))),
            "{output}"
        );
        assert!(
            !hints
                .iter()
                .any(|hint| hint.as_str().is_some_and(|s| s.contains("access rights"))),
            "{output}"
        );
        let related = &output["related"][0];
        assert_eq!(related["tool"], "graphBuild", "{output}");
        let input = related["input"]["queries"][0].clone();
        let recovered = call(&dir.0, ToolId::GraphBuild, input);
        let expected =
            std::fs::canonicalize(dir.0.join(row["path"].as_str().expect("path"))).expect("scope");
        assert_eq!(recovered["root"], expected.to_string_lossy().as_ref());
    }
}

#[test]
fn related_queries_preserve_scope_without_carrying_node_kind_filters() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "import {b} from './b'; export const a = b;");
    dir.write("app/b.ts", "export const b = 1;");
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    let node = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"node", "path":"app", "target":"a.ts", "kind":"file"}),
    );
    for related in node["related"].as_array().expect("related queries") {
        let row = &related["input"]["queries"][0];
        assert!(row.get("kind").is_none());
        let output = call(&dir.0, ToolId::GraphQuery, row.clone());
        assert_eq!(output["graph"]["id"], node["graph"]["id"]);
        assert!(output.get("error").is_none(), "{output}");
    }
}

#[test]
fn cancelled_build_does_not_publish() {
    let dir = Fixture::new();
    dir.write("a.ts", "export const a = 1;");
    let ctx = context();
    ctx.cancellation.cancel();
    assert!(
        execute(
            ToolId::GraphBuild,
            &json!({}),
            &workspace_policy(&dir.0),
            &ContentSecurity::new(),
            &ctx,
            None
        )
        .is_err()
    );
    assert!(!dir.0.join(".octocode/graph").exists());
}

#[test]
fn cycle_member_continuations_validate_without_hiding_public_target_inputs() {
    let dir = Fixture::new();
    for i in 0..51 {
        dir.write(
            &format!("cycle/f{i}.ts"),
            format!("import './f{}'; export const f{i} = {i};", (i + 1) % 51),
        );
    }
    call(&dir.0, ToolId::GraphBuild, json!({"path":"cycle"}));
    let page = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"cycles", "path":"cycle"}),
    );
    let preview = page["results"][0]["nodes"]
        .as_array()
        .expect("cycle preview");
    let row = &page["next"]["cycle0"]["input"]["queries"][0];
    assert!(row["cycle"].is_string());
    assert!(row.get("target").is_none());
    let rest = call(&dir.0, ToolId::GraphQuery, row.clone());
    let remaining = rest["results"].as_array().expect("remaining cycle members");
    assert_eq!(preview.len() + remaining.len(), 51);
    let unique = preview
        .iter()
        .chain(remaining)
        .map(Value::to_string)
        .collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), 51);
}

#[test]
fn simultaneous_builds_publish_one_snapshot_and_reuse_it() {
    let dir = Fixture::new();
    dir.write("a.ts", "export const a = 1;");
    let outputs = std::thread::scope(|scope| {
        let tasks = (0..3)
            .map(|_| scope.spawn(|| call(&dir.0, ToolId::GraphBuild, json!({}))))
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| task.join().expect("build"))
            .collect::<Vec<_>>()
    });
    assert!(outputs.iter().all(|out| out["id"] == outputs[0]["id"]));
    assert_eq!(list_snapshots(&dir.0.join(".octocode/graph")).len(), 1);
}

#[cfg(unix)]
#[test]
fn storage_symlink_cannot_escape_workspace() {
    let dir = Fixture::new();
    let outside = Fixture::new();
    std::os::unix::fs::symlink(&outside.0, dir.0.join(".octocode")).expect("symlink");
    let out = call(&dir.0, ToolId::GraphBuild, json!({}));
    assert!(out["errorCode"].is_string());
    assert!(!outside.0.join("graph").exists());
}

#[cfg(unix)]
#[test]
fn build_lock_symlink_cannot_create_an_outside_file() {
    let dir = Fixture::new();
    let outside = Fixture::new();
    let home = dir.0.join(".octocode/graph");
    std::fs::create_dir_all(&home).expect("graph home");
    let target = outside.0.join("must-not-exist");
    std::os::unix::fs::symlink(&target, home.join(".build.lock")).expect("symlink");
    let out = call(&dir.0, ToolId::GraphBuild, json!({}));
    assert!(out["errorCode"].is_string());
    assert!(!target.exists());
}

mod candidates;

mod constructors;

mod projection;
