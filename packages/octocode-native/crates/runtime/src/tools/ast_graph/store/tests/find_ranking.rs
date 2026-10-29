//! Entering the graph: `find` ranks natural text by identifier words, and an
//! unknown reference suggests the same ranking.
use super::*;

/// `fixture()` plus names that share words across cases and separators.
fn words_fixture() -> Fixture {
    let dir = fixture();
    write_under(
        &dir,
        "app/src",
        &[
            (
                "config/loader.ts",
                "export function parseConfig(text: string) { return text; }\nexport class ConfigLoader {\n  load() { return parseConfig('x'); }\n}\n",
            ),
            (
                "snapshot.ts",
                "export class SnapshotReader {\n  read() { return 1; }\n}\nexport class SnapshotReaderOptions {}\nexport function readSnapshot() { return new SnapshotReader().read(); }\n",
            ),
            (
                "pool.ts",
                "export class OriginPool {\n  read() { return 2; }\n  write() { return 3; }\n}\n",
            ),
        ],
    );
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    dir
}

fn find(dir: &Fixture, text: &str) -> GraphOutput {
    ask(dir.0.as_path(), "find", Some(text), |o| o.limit = Some(100))
}

#[test]
fn find_matches_every_word_of_natural_text() {
    let dir = words_fixture();
    let out = find(&dir, "snapshot reader");
    assert_eq!(
        ids(&out),
        vec![
            "src/snapshot.ts#SnapshotReader",
            "src/snapshot.ts#SnapshotReaderOptions",
            "src/snapshot.ts#SnapshotReader.read",
        ],
        "{}",
        out.value
    );
    let first = &out.value["results"][0];
    assert_eq!(first["match"], "words", "{first}");
    assert!(first.get("matchedWords").is_none(), "{first}");
}

#[test]
fn find_splits_case_separators_and_qualified_names() {
    let dir = words_fixture();
    let snake = find(&dir, "parse_config");
    assert_eq!(
        ids(&snake).first().map(String::as_str),
        Some("src/config/loader.ts#parseConfig"),
        "{}",
        snake.value
    );
    let member = find(&dir, "OriginPool read");
    assert_eq!(
        ids(&member).first().map(String::as_str),
        Some("src/pool.ts#OriginPool.read"),
        "{}",
        member.value
    );
    let loader = find(&dir, "parse config loader");
    assert!(
        ids(&loader).contains(&"src/config/loader.ts#parseConfig".to_owned()),
        "every word appears in the key: {}",
        loader.value
    );
}

#[test]
fn find_without_a_full_match_returns_the_best_partial_coverage() {
    let dir = words_fixture();
    let out = find(&dir, "parse config yaml");
    assert_eq!(
        ids(&out).first().map(String::as_str),
        Some("src/config/loader.ts#parseConfig"),
        "{}",
        out.value
    );
    for row in out.value["results"].as_array().expect("rows") {
        assert_eq!(row["matchedWords"], json!(["parse", "config"]), "{row}");
    }
}

#[test]
fn find_single_word_keeps_its_population_and_ranks_exact_names_by_callers() {
    let dir = words_fixture();
    let out = find(&dir, "read");
    let rows = out.value["results"].as_array().expect("rows");
    let expected = ["exact", "exact", "words", "words"];
    assert_eq!(
        rows.iter()
            .map(|row| row["match"].as_str().unwrap())
            .collect::<Vec<_>>()[..4],
        expected,
        "{}",
        out.value
    );
    // `SnapshotReader.read` has a caller; `OriginPool.read` has none.
    assert_eq!(
        ids(&out)[..2],
        [
            "src/snapshot.ts#SnapshotReader.read",
            "src/pool.ts#OriginPool.read"
        ]
    );
    // The population is still every name or key containing the text.
    let all = ask(dir.0.as_path(), "find", Some("a"), |o| o.limit = Some(1000));
    let containing = all.value["results"]
        .as_array()
        .expect("rows")
        .iter()
        .filter(|row| {
            let id = row["id"].as_str().unwrap_or_default().to_lowercase();
            let name = row["name"].as_str().unwrap_or(&id).to_lowercase();
            name.contains("read") || id.contains("read")
        })
        .count();
    assert_eq!(out.value["total"], containing, "{}", out.value);
}

#[test]
fn unknown_reference_suggests_ranked_find_matches() {
    let dir = words_fixture();
    let out = ask(dir.0.as_path(), "node", Some("Snapshot Reader"), |_| {});
    assert_eq!(out.value["errorCode"], "notFound", "{}", out.value);
    assert_eq!(
        out.value["suggestions"][0], "src/snapshot.ts#SnapshotReader",
        "{}",
        out.value
    );
}

#[test]
fn every_listed_detector_runs_when_selected_alone() {
    let dir = issues_fixture();
    write_under(
        &dir,
        "app/src",
        &[(
            "local.ts",
            "export function helper() { return 1; }\nexport const value = helper();\n",
        )],
    );
    let main = dir.0.as_path().join("app/src/main.ts");
    let text = std::fs::read_to_string(&main).expect("main");
    std::fs::write(&main, format!("import './local';\n{text}")).expect("entry");
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let alone = ask(dir.0.as_path(), "issues", None, |o| {
        o.detectors = vec!["export-only-local".into()];
        o.min_tier = Some(0);
    });
    assert!(
        findings(&alone, "export-only-local").contains(&"src/local.ts".to_owned()),
        "{}",
        alone.value
    );
}

#[test]
fn find_ranks_underscore_names_below_equally_matched_public_names() {
    let dir = fixture();
    write_under(
        &dir,
        "app/src",
        &[(
            "loads.ts",
            "export function _loadUser() { return 1; }\nexport function loadItem() { return 2; }\nexport function run() { return _loadUser() + _loadUser() + loadItem(); }\n",
        )],
    );
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = find(&dir, "load");
    assert_eq!(
        ids(&out)[..2],
        ["src/loads.ts#loadItem", "src/loads.ts#_loadUser"],
        "{}",
        out.value
    );
}
