//! Entering the graph: `find` ranks natural text by identifier words, and an
//! unknown reference suggests the same ranking.
use super::*;
use octocode_test_support::json_fixture;

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
            "app/src/snapshot.ts#SnapshotReader",
            "app/src/snapshot.ts#SnapshotReaderOptions",
            "app/src/snapshot.ts#SnapshotReader.read",
        ],
        "{}",
        out.value
    );
    let first = &out.value["rows"][0];
    assert_eq!(first["match"], "words", "{first}");
    assert!(first.get("matchedWords").is_none(), "{first}");
}

#[test]
fn find_splits_case_separators_and_qualified_names() {
    let dir = words_fixture();
    let snake = find(&dir, "parse_config");
    assert_eq!(
        ids(&snake).first().map(String::as_str),
        Some("app/src/config/loader.ts#parseConfig"),
        "{}",
        snake.value
    );
    let member = find(&dir, "OriginPool read");
    assert_eq!(
        ids(&member).first().map(String::as_str),
        Some("app/src/pool.ts#OriginPool.read"),
        "{}",
        member.value
    );
    let loader = find(&dir, "parse config loader");
    assert!(
        ids(&loader).contains(&"app/src/config/loader.ts#parseConfig".to_owned()),
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
        Some("app/src/config/loader.ts#parseConfig"),
        "{}",
        out.value
    );
    for row in out.value["rows"].as_array().expect("rows") {
        assert_eq!(
            row["matchedWords"],
            json_fixture(r#"["parse", "config"]"#),
            "{row}"
        );
    }
}

#[test]
fn find_single_word_keeps_its_population_and_ranks_exact_names_by_callers() {
    let dir = words_fixture();
    let out = find(&dir, "read");
    let rows = out.value["rows"].as_array().expect("rows");
    // Rows come in tier order, exact names first; a word match states
    // `prefix` when its name starts with the text.
    let tiers: [&[&str]; 4] = [&["exact"], &["prefix", "words"], &["contains"], &["path"]];
    let ranks = rows
        .iter()
        .map(|row| {
            tiers
                .iter()
                .position(|tier| tier.iter().any(|label| row["match"] == *label))
                .expect("a known tier")
        })
        .collect::<Vec<_>>();
    assert!(ranks.is_sorted(), "{}", out.value);
    assert_eq!(ranks[..2], [0, 0], "{}", out.value);
    // `SnapshotReader.read` has a caller; `OriginPool.read` has none.
    assert_eq!(
        ids(&out)[..2],
        [
            "app/src/snapshot.ts#SnapshotReader.read",
            "app/src/pool.ts#OriginPool.read"
        ]
    );
    // The population is still every name or key containing the text.
    let all = ask(dir.0.as_path(), "find", Some("a"), |o| o.limit = Some(1000));
    let containing = all.value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .filter(|row| {
            let id = row["id"].as_str().unwrap_or_default().to_lowercase();
            let name = row["symbolName"].as_str().unwrap_or(&id).to_lowercase();
            name.contains("read") || id.contains("read")
        })
        .count();
    assert_eq!(
        out.value["pagination"]["total"], containing,
        "{}",
        out.value
    );
}

#[test]
fn unknown_reference_suggests_ranked_find_matches() {
    let dir = words_fixture();
    let out = ask(dir.0.as_path(), "node", Some("Snapshot Reader"), |_| {});
    assert_eq!(out.value["errorCode"], "notFound", "{}", out.value);
    assert_eq!(
        out.value["suggestions"][0], "app/src/snapshot.ts#SnapshotReader",
        "{}",
        out.value
    );
}

/// A near name stands in for a reference no node has: one typo of the whole
/// name (`runCLl`), or names whose words spell most of the reference, each
/// word as written, a prefix or one typo away (`wrteJsonFle`). A node that
/// spells less of it (one short word of `zzqxNoSuchFn`) shares a word, not
/// the name: nothing stands in, and the find lead searches for it.
#[test]
fn an_unknown_reference_suggests_only_near_names() {
    let dir = words_fixture();
    write_under(
        &dir,
        "app/src",
        &[(
            "cli.ts",
            "export function runCLI() { return 1; }\nexport function writeJson() { return runCLI(); }\nexport function node() { return 2; }\nexport function now() { return 3; }\n",
        )],
    );
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();
    for (reference, near) in [
        ("runCLl", "app/src/cli.ts#runCLI"),
        ("wrteJsonFle", "app/src/cli.ts#writeJson"),
        ("zzqxSnapshotReader", "app/src/snapshot.ts#SnapshotReader"),
    ] {
        let out = ask(root, "callers", Some(reference), |_| {});
        assert_eq!(out.value["errorCode"], "notFound", "{}", out.value);
        assert_eq!(
            out.value["suggestions"][0], near,
            "{reference}: {}",
            out.value
        );
    }
    for reference in ["zzqxNoSuchFn", "zzqxSnapshotSuchFn"] {
        let out = ask(root, "callers", Some(reference), |_| {});
        assert_eq!(out.value["errorCode"], "notFound", "{}", out.value);
        assert!(
            out.value.get("suggestions").is_none(),
            "{reference}: {}",
            out.value
        );
        assert_eq!(out.value["find"], reference, "{}", out.value);
    }
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
        findings(&alone, "export-only-local").contains(&"app/src/local.ts".to_owned()),
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
        ["app/src/loads.ts#loadItem", "app/src/loads.ts#_loadUser"],
        "{}",
        out.value
    );
}

/// `fixture()` plus `src/util/parse.ts` declaring `parse` and `helper`.
fn parse_fixture() -> Fixture {
    let dir = fixture();
    write_under(
        &dir,
        "app/src/util",
        &[(
            "parse.ts",
            "export function helper(x: string) { return x; }\nexport function parse(t: string) { return helper(t); }\n",
        )],
    );
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    dir
}

/// A query word a node's name matches keeps that node even when another
/// node matches more words only through its file path; the path match
/// still ranks first, and the partial row names the words it matched.
#[test]
fn find_keeps_name_matches_that_path_words_outnumber() {
    let dir = parse_fixture();
    let out = find(&dir, "parse helper");
    let found = ids(&out);
    let at = |id: &str| found.iter().position(|row| row == id);
    let helper = at("app/src/util/parse.ts#helper").expect("helper");
    let parse = at("app/src/util/parse.ts#parse").expect("parse matches a word by name");
    assert!(helper < parse, "{}", out.value);
    assert_eq!(
        out.value["rows"][parse]["matchedWords"],
        json_fixture(r#"["parse"]"#),
        "{}",
        out.value
    );
}

/// A word match whose name starts with the whole query says `prefix`.
#[test]
fn find_labels_whole_name_prefixes() {
    let dir = parse_fixture();
    let out = find(&dir, "pars");
    let rows = out.value["rows"].as_array().expect("rows");
    let parse = rows
        .iter()
        .find(|row| row["id"] == "app/src/util/parse.ts#parse")
        .expect("parse");
    assert_eq!(parse["match"], "prefix", "{}", out.value);
}
