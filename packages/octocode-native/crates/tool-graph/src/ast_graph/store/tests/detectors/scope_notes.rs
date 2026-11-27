//! Dead-code evidence that depends on what the snapshot's root holds: names
//! loaders build paths from, and manifests above the root.
use super::*;
use octocode_test_support::json_fixture;

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
            .find(|row| row["subject"] == format!("app/{subject}"))
            .cloned()
            .unwrap_or_else(|| panic!("{subject}: {}", issues.value))
    };
    let named = row("src/checks/late-check.ts");
    assert_eq!(named["tier"], 60, "{named}");
    assert_eq!(
        named["evidence"]["mentionedIn"],
        json_fixture(r#"["app/src/catalog.ts:1"]"#),
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
    // The notes, without what rows of a detector share (`shared`): a row's
    // evidence may name the manifest its entrypoints came from.
    let notes = |snapshot: &GraphOutput| {
        let id = snapshot.value["graph"].as_str().expect("id").to_owned();
        let notes = ask(root, "issues", None, |o| {
            o.graph = Some(id);
            o.min_tier = Some(0);
        })
        .value["summary"]["notes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Value::Array(
            notes
                .into_iter()
                .filter(|note| note.get("shared").is_none())
                .collect(),
        )
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

/// A snapshot below its package root reads neither the package's entries
/// nor its references outside the snapshot (tests, scripts, docs), so none
/// of its dead-code findings claims tier 90 or above: each is at most tier
/// 60, the default view lists none, and `related` carries the build of the
/// package root, runnable as given. That build judges the export only a test
/// outside the snapshot imports below 90 and the export nothing names at 90
/// or above, and offers no wider build.
#[test]
fn a_snapshot_below_its_package_root_states_no_tier_90_dead_code_and_leads_to_the_root() {
    const DEAD_CODE: [&str; 4] = [
        "unreachable-file",
        "unused-export",
        "export-only-local",
        "test-only",
    ];
    let dir = git_app(&[
        ("package.json", r#"{"name":"app","main":"src/main.ts"}"#),
        (
            "src/main.ts",
            "import { used } from './lib';\nexport const main = () => used();\n",
        ),
        (
            "src/lib.ts",
            "export function used() { return 1; }\nexport function testedOutside() { return 2; }\nexport function unusedAnywhere() { return 3; }\n",
        ),
        (
            "test/lib.test.ts",
            "import { testedOutside } from '../src/lib';\ntestedOutside();\n",
        ),
    ]);
    let root = dir.0.as_path();
    let src = ingest_root(root, &root.join("app/src"), None, false);
    assert_eq!(src.class, ExitClass::Success, "{}", src.value);
    let issues = |id: &str, min_tier: Option<u8>| {
        ask(root, "issues", None, |o| {
            o.graph = Some(id.to_owned());
            o.min_tier = min_tier;
            o.limit = Some(200);
        })
    };
    let rows = |out: &GraphOutput| crate::ast_graph::test_support::issue_rows(&out.value);
    let export_tier = |out: &GraphOutput, name: &str| {
        rows(out)
            .iter()
            .filter(|row| row["detector"] == "unused-export")
            .find(|row| {
                row["evidence"]["exports"]
                    .as_array()
                    .is_some_and(|names| names.iter().any(|n| n == name))
            })
            .and_then(|row| row["tier"].as_u64())
    };
    let src_id = src.value["graph"].as_str().expect("id").to_owned();

    let listed = issues(&src_id, None);
    assert!(
        rows(&listed)
            .iter()
            .all(|row| !DEAD_CODE.iter().any(|d| row["detector"] == *d)),
        "the default view states dead code below the package root: {}",
        listed.value
    );
    let every = issues(&src_id, Some(0));
    assert!(
        export_tier(&every, "unusedAnywhere").is_some(),
        "{}",
        every.value
    );
    for row in rows(&every) {
        if DEAD_CODE.iter().any(|d| row["detector"] == *d) {
            assert!(row["tier"].as_u64() <= Some(60), "{row}");
        }
    }
    let lead = |out: &GraphOutput| {
        out.value["related"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|call| call["tool"] == "graphBuild")
            .cloned()
    };
    for out in [&listed, &every] {
        let call = lead(out).unwrap_or_else(|| panic!("no graphBuild lead: {}", out.value));
        assert_eq!(call["input"]["queries"][0]["path"], "app", "{call}");
    }

    // The lead, run as given, builds the package root.
    let call = lead(&listed).expect("lead");
    let path = call["input"]["queries"][0]["path"].as_str().expect("path");
    let app = ingest_root(root, &root.join(path), None, false);
    assert_eq!(app.class, ExitClass::Success, "{}", app.value);
    let app_id = app.value["graph"].as_str().expect("id").to_owned();
    let judged = issues(&app_id, Some(0));
    assert!(
        export_tier(&judged, "unusedAnywhere") >= Some(90),
        "{}",
        judged.value
    );
    assert!(
        export_tier(&judged, "testedOutside") < Some(90),
        "{}",
        judged.value
    );
    assert!(lead(&judged).is_none(), "{}", judged.value);
}
