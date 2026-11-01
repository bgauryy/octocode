//! Graph-quality plan: call coverage, string references and member calls.
use super::*;

#[test]
fn plan_1_call_answers_report_coverage() {
    let dir = fixture();
    write_under(
        &dir,
        "app/src",
        &[(
            "opener.ts",
            "import { Box } from './lib';\nfunction make(): any { return null; }\nexport function run() {\n  const b = make();\n  b.open();\n  return new Box();\n}\n",
        )],
    );
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(
        dir.0.as_path(),
        "callers",
        Some("src/lib.ts#Box.open"),
        |_| {},
    );
    let coverage = &out.value["coverage"];
    let recall = coverage["linkage"]["ratio"].as_f64().expect("recall");
    assert!(recall < 1.0, "{}", out.value);
    assert!(
        coverage["warning"].as_str().is_some_and(|w| !w.is_empty()),
        "{}",
        out.value
    );
    let impact = ask(
        dir.0.as_path(),
        "impact",
        Some("src/lib.ts#Box.open"),
        |_| {},
    );
    assert!(
        impact.value["coverage"]["linkage"]["ratio"].is_number(),
        "{}",
        impact.value
    );
}

#[test]
fn plan_2_string_referenced_files_are_low_tier_and_hidden_by_default() {
    let dir = issues_fixture();
    write_under(
        &dir,
        "app",
        &[
            ("src/worker.ts", "export const work = () => 1;\n"),
            (
                "src/spawn.ts",
                "export const spawn = () => new Worker(new URL('./worker.ts', import.meta.url));\n",
            ),
            ("src/tool.ts", "export const tool = 1;\n"),
            ("build.config.json", "{\"entry\": \"./src/tool.ts\"}\n"),
            (
                "src/local.ts",
                "export function helper() { return 1; }\nexport const value = helper();\n",
            ),
        ],
    );
    // Reached from the entry: an unreachable file's exports are not judged
    // apart from the file.
    let main = dir.0.as_path().join("app/src/main.ts");
    let text = std::fs::read_to_string(&main).expect("main");
    std::fs::write(&main, format!("import './local';\n{text}")).expect("entry");
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let default = ask(dir.0.as_path(), "issues", None, |o| o.limit = Some(500));
    let shown = findings(&default, "unreachable-file");
    assert!(
        !shown.contains(&"src/worker.ts".to_owned()),
        "string-referenced worker hidden: {shown:?}"
    );
    assert!(
        !shown.contains(&"src/tool.ts".to_owned()),
        "config-referenced file hidden: {shown:?}"
    );
    assert!(
        shown.contains(&"src/orphan.ts".to_owned()),
        "true orphan still shown: {shown:?}"
    );
    let orphan = default.value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["subject"] == "src/orphan.ts" && r["detector"] == "unreachable-file")
        .expect("orphan");
    assert!(orphan["tier"].as_u64().unwrap_or(0) >= 90, "{orphan}");

    let all = ask(dir.0.as_path(), "issues", None, |o| {
        o.limit = Some(500);
        o.min_tier = Some(0);
    });
    let worker = all.value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["subject"] == "src/worker.ts" && r["detector"] == "unreachable-file")
        .expect("worker listed at tier 60");
    assert_eq!(worker["tier"], 60, "{worker}");
    assert!(
        worker["evidence"]["mentionedIn"]
            .as_array()
            .is_some_and(|m| !m.is_empty()),
        "{worker}"
    );

    // An export used only inside its own file is "could be un-exported".
    assert!(
        findings(&all, "export-only-local").contains(&"src/local.ts".to_owned()),
        "{}",
        all.value["summary"]
    );
    let unused = findings(&all, "unused-export");
    assert!(
        !unused.contains(&"src/local.ts".to_owned()) || {
            let row = all.value["rows"]
                .as_array()
                .expect("rows")
                .iter()
                .find(|r| r["subject"] == "src/local.ts" && r["detector"] == "unused-export")
                .expect("row");
            !row["evidence"]["exports"]
                .as_array()
                .expect("exports")
                .contains(&json!("helper"))
        },
        "helper is used locally, not unused"
    );
}

#[test]
fn plan_3_type_qualified_calls_and_scoped_disambiguation_link() {
    let dir = git_app(&[
        (
            "com/shop/Cart.java",
            "package com.shop;\npublic class Cart {\n  public static Cart empty() { return new Cart(); }\n  public void add() {}\n}\n",
        ),
        (
            "com/shop/Checkout.java",
            "package com.shop;\npublic class Checkout {\n  public void run() {\n    Cart c = Cart.empty();\n    Helper.assist();\n  }\n}\n",
        ),
        (
            "com/shop/Helper.java",
            "package com.shop;\npublic class Helper {\n  public static void assist() {}\n}\n",
        ),
        (
            "com/other/Helper.java",
            "package com.other;\npublic class Helper {\n  public static void assist() {}\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let callees = ask(
        dir.0.as_path(),
        "callees",
        Some("com/shop/Checkout.java#Checkout.run"),
        |o| o.confidence = Some("medium".into()),
    );
    assert!(
        callees.value["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["confidence"] == "medium")
    );
    let strong = ask(
        dir.0.as_path(),
        "callees",
        Some("com/shop/Checkout.java#Checkout.run"),
        |o| o.confidence = Some("high".into()),
    );
    assert_eq!(
        strong.value["pagination"]["totalItems"], 0,
        "same-package bindings remain candidates: {}",
        strong.value
    );
    let got = ids(&callees);
    assert!(
        got.contains(&"com/shop/Cart.java#Cart.empty".to_owned()),
        "{}",
        callees.value
    );
    assert!(
        got.contains(&"com/shop/Helper.java#Helper.assist".to_owned()),
        "same-package wins: {got:?}"
    );
    assert!(
        !got.contains(&"com/other/Helper.java#Helper.assist".to_owned()),
        "{got:?}"
    );
}

#[test]
fn plan_4_receiver_typed_member_calls_link() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod store;\npub mod service;\n"),
        (
            "src/store.rs",
            "pub struct Store;\nimpl Store {\n    pub fn new() -> Self { Store }\n    pub fn save(&self) {}\n}\npub struct Other;\nimpl Other {\n    pub fn save(&self) {}\n}\n",
        ),
        (
            "src/service.rs",
            "use crate::store::Store;\npub fn run() {\n    let s = Store::new();\n    s.save();\n}\npub fn typed(s: &Store) {\n    s.save();\n}\n",
        ),
        (
            "web/store.ts",
            "export class Cart {\n  save() { return 1; }\n}\nexport class Box {\n  save() { return 2; }\n}\n",
        ),
        (
            "web/app.ts",
            "import { Cart } from './store';\nexport function go() {\n  const c = new Cart();\n  c.save();\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let rust = ask(
        dir.0.as_path(),
        "callees",
        Some("src/service.rs#run"),
        |_| {},
    );
    let rows = rust.value["rows"].as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|r| r["id"] == "src/store.rs#Store.save" && r["via"] == "receiver-type"),
        "{}",
        rust.value
    );
    assert!(!ids(&rust).contains(&"src/store.rs#Other.save".to_owned()));
    let typed = ask(
        dir.0.as_path(),
        "callees",
        Some("src/service.rs#typed"),
        |_| {},
    );
    assert!(
        ids(&typed).contains(&"src/store.rs#Store.save".to_owned()),
        "{}",
        typed.value
    );
    let ts = ask(dir.0.as_path(), "callees", Some("web/app.ts#go"), |_| {});
    assert!(
        ids(&ts).contains(&"web/store.ts#Cart.save".to_owned()),
        "{}",
        ts.value
    );
    assert!(!ids(&ts).contains(&"web/store.ts#Box.save".to_owned()));
}

#[test]
fn plan_4_typed_this_field_calls_are_member_calls_not_local_matches() {
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.as_path().join(".git")).expect("git");
    write_under(
        &dir,
        "app/src",
        &[
            (
                "repo.ts",
                "export class Repo {\n  load() { return 1; }\n}\n",
            ),
            (
                "svc.ts",
                "import { Repo } from './repo';\nfunction load() { return 0; }\nexport class Svc {\n  private repo: Repo = new Repo();\n  run() {\n    return this.repo.load();\n  }\n}\nexport const unused = load();\n",
            ),
        ],
    );
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(
        dir.0.as_path(),
        "callees",
        Some("src/svc.ts#Svc.run"),
        |_| {},
    );
    let got = ids(&out);
    assert!(
        got.contains(&"src/repo.ts#Repo.load".to_owned()),
        "{}",
        out.value
    );
    assert!(
        !got.contains(&"src/svc.ts#load".to_owned()),
        "typed member call must not match the local fn: {got:?}"
    );
}
