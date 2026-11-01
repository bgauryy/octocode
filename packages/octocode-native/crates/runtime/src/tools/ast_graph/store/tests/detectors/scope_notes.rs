//! Dead-code evidence that depends on what the snapshot's root holds: names
//! loaders build paths from, and manifests above the root.
use super::*;

/// A file named by a quoted string (a catalog `['late-check']` whose loader
/// imports `./checks/${name}.js`) is mentioned, so it is no tier-100
/// orphan; an orphan nothing names stays tier 100.
#[test]
fn a_file_named_by_a_quoted_string_is_mentioned_not_an_orphan() {
    let dir = git_app(&[
        ("package.json", r#"{"name":"app","main":"src/main.ts"}"#),
        (
            "src/main.ts",
            "import { names } from './catalog';\nexport const main = () => names.map((name) => import(`./checks/${name}.js`));\n",
        ),
        ("src/catalog.ts", "export const names = ['late-check'];\n"),
        ("src/checks/late-check.ts", "export const run = () => 1;\n"),
        ("src/lonely.ts", "export const lonely = 1;\n"),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| {
        o.detectors = vec!["unreachable-file".into()];
        o.min_tier = Some(0);
    });
    let row = |subject: &str| {
        issues.value["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| row["subject"] == subject)
            .cloned()
            .unwrap_or_else(|| panic!("{subject}: {}", issues.value))
    };
    let named = row("src/checks/late-check.ts");
    assert_eq!(named["tier"], 60, "{named}");
    assert_eq!(
        named["evidence"]["mentionedIn"],
        json!(["src/catalog.ts:1"]),
        "{named}"
    );
    assert_eq!(row("src/lonely.ts")["tier"], 100, "{}", issues.value);
}

/// A snapshot of a directory below its package manifest infers entrypoints
/// from file conventions only: `issues` names the ancestor holding the
/// manifest. A snapshot of the package itself carries no such note.
#[test]
fn issues_name_the_manifest_above_a_subdirectory_snapshot() {
    let dir = dead_code_app();
    let root = dir.0.as_path();
    let src = ingest_root(root, &root.join("app/src"), None, false);
    assert_eq!(src.class, ExitClass::Success, "{}", src.value);
    let app = ingest_fixture(root, None);
    assert_eq!(app.class, ExitClass::Success, "{}", app.value);
    let notes = |snapshot: &GraphOutput| {
        let id = snapshot.value["id"].as_str().expect("id").to_owned();
        ask(root, "issues", None, |o| {
            o.graph = Some(id);
            o.min_tier = Some(0);
        })
        .value["summary"]["notes"]
            .clone()
    };
    let below = notes(&src).to_string();
    assert!(
        below.contains("package.json") && below.contains("graphBuild") && below.contains("app"),
        "{below}"
    );
    assert!(
        !notes(&app).to_string().contains("package.json"),
        "{}",
        notes(&app)
    );
}
