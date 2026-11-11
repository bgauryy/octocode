//! Issue hypotheses: ranking, evidence, baselines, entrypoints and drift.
use super::*;

#[test]
fn issues_rank_hypotheses_with_evidence_and_controls() {
    let dir = issues_fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();
    let out = ask(root, "issues", None, |o| o.limit = Some(200));
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);

    assert_eq!(findings(&out, "cycle"), vec!["src/a.ts"], "{}", out.value);
    let cycle = out.value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["detector"] == "cycle")
        .expect("cycle finding");
    assert_eq!(cycle["evidence"]["cutCount"], 1);
    assert_eq!(
        cycle["evidence"]["witness"].as_array().map(Vec::len),
        Some(2)
    );

    let unreachable = findings(&out, "unreachable-file");
    assert!(
        unreachable.contains(&"src/orphan.ts".to_owned()),
        "{unreachable:?}"
    );
    assert!(
        !unreachable.contains(&"src/client.gen.ts".to_owned()),
        "generated files are never subjects"
    );
    assert!(!unreachable.contains(&"src/main.ts".to_owned()));

    assert_eq!(findings(&out, "test-only"), vec!["src/fmt.ts"]);
    assert_eq!(
        findings(&out, "undeclared-dependency"),
        vec!["app::left-pad"]
    );

    let unused = findings(&out, "unused-export");
    assert!(
        unused.contains(&"src/lib.ts".to_owned()),
        "Box is never imported: {unused:?}"
    );

    let scores = out.value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["score"].as_f64().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(
        scores.windows(2).all(|w| w[0] >= w[1]),
        "ranked by score: {scores:?}"
    );

    let only = ask(root, "issues", None, |o| o.detectors = vec!["cycle".into()]);
    assert_eq!(only.value["pagination"]["totalItems"], 1);
    let bad = ask(root, "issues", None, |o| o.detectors = vec!["nope".into()]);
    assert_eq!(bad.class, ExitClass::InvalidInput);
}

/// Finding evidence lists every member, export and site it counts: a list
/// is never cut to a sample beside its count.
#[test]
fn issue_evidence_lists_every_item_it_counts() {
    let dir = issues_fixture();
    let src = dir.0.as_path().join("app/src");
    for n in 0..25 {
        std::fs::write(
            src.join(format!("ring{n}.ts")),
            format!(
                "import {{ r{next} }} from './ring{next}';\nimport pad from 'left-pad';\nexport const r{n} = () => r{next}() + pad('');\n",
                next = (n + 1) % 25
            ),
        )
        .expect("ring file");
    }
    let exports = (0..25)
        .map(|n| format!("export const unusedValue{n} = {n};\n"))
        .collect::<String>();
    std::fs::write(src.join("many.ts"), exports).expect("exports");
    // The entry reaches both: an unreachable file is one `unreachable-file`
    // finding, its exports not listed again.
    let main = std::fs::read_to_string(src.join("main.ts")).expect("main");
    std::fs::write(
        src.join("main.ts"),
        format!("import './many';\nimport './ring0';\n{main}"),
    )
    .expect("entry");
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "issues", None, |o| o.limit = Some(500));
    let rows = out.value["rows"].as_array().cloned().unwrap_or_default();
    let finding = |detector: &str, subject: &str| {
        rows.iter()
            .find(|row| {
                row["detector"] == detector
                    && row["subject"].as_str().is_some_and(|s| s.contains(subject))
            })
            .cloned()
            .unwrap_or_else(|| panic!("{detector} {subject}: {}", out.value))
    };
    let cycle = finding("cycle", "ring");
    assert_eq!(cycle["evidence"]["size"], 25, "{cycle}");
    assert_eq!(
        cycle["evidence"]["members"].as_array().map(Vec::len),
        Some(25),
        "{cycle}"
    );
    let unused = finding("unused-export", "many.ts");
    assert_eq!(
        unused["evidence"]["exports"].as_array().map(Vec::len),
        Some(25),
        "{unused}"
    );
    let undeclared = finding("undeclared-dependency", "left-pad");
    let sites = undeclared["evidence"]["sites"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    assert!(sites >= 26, "{undeclared}");
}

#[test]
fn issues_baseline_reports_new_and_resolved_findings() {
    let dir = issues_fixture();
    let first = ingest_fixture(dir.0.as_path(), None);
    let base = first.value["id"].as_str().expect("id").to_owned();
    dir.write("app/src/b.ts", "export const b = () => 1;\n");
    dir.write("app/src/orphan2.ts", "export const x = 1;\n");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "issues", None, |o| {
        o.baseline = Some(base.clone());
        o.limit = Some(200);
    });
    let baseline = &out.value["baseline"];
    assert!(
        baseline["resolved"].as_u64().unwrap_or(0) >= 1,
        "{baseline}"
    );
    assert!(
        baseline["resolvedFindings"]
            .as_array()
            .expect("resolved")
            .iter()
            .any(|f| f["detector"] == "cycle"),
        "{baseline}"
    );
    let new = out.value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .filter(|row| row["status"] == "new")
        .map(|row| row["subject"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(new.contains(&"src/orphan2.ts"), "{new:?}");
}

/// The pages of an `issues` listing slice one listing: page 1 is truncated
/// and the next page states the same total.
#[test]
fn issues_pages_slice_one_listing() {
    let dir = issues_fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let first = ask(dir.0.as_path(), "issues", None, |o| {
        o.limit = Some(1);
        o.min_tier = Some(0);
    });
    assert_eq!(
        first.value["pagination"]["hasMore"], true,
        "{}",
        first.value
    );
    let second = ask(dir.0.as_path(), "issues", None, |o| {
        o.limit = Some(1);
        o.offset = Some(1);
        o.min_tier = Some(0);
    });
    assert_eq!(
        second.value["pagination"]["totalItems"],
        first.value["pagination"]["totalItems"]
    );
}

/// `issues --entry` names the entrypoints in place of the inferred ones:
/// reachability and unused exports are judged from them, and an entry that
/// matches no node is a not-found error.
#[test]
fn issues_take_explicit_entrypoints_in_place_of_the_inferred_ones() {
    let dir = issues_fixture();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let inferred = ask(root, "issues", None, |o| {
        o.detectors = vec!["unreachable-file".into(), "test-only".into()];
        o.limit = Some(200);
        o.min_tier = Some(0);
    });
    assert!(findings(&inferred, "unreachable-file").contains(&"src/orphan.ts".to_owned()));
    let explicit = ask(root, "issues", None, |o| {
        o.detectors = vec!["unreachable-file".into(), "test-only".into()];
        o.entries = vec!["src/orphan.ts".into()];
        o.limit = Some(200);
        o.min_tier = Some(0);
    });
    let unreachable = findings(&explicit, "unreachable-file");
    assert!(
        !unreachable.contains(&"src/orphan.ts".to_owned()),
        "{}",
        explicit.value
    );
    // main.ts is no entry now: only its test reaches it.
    assert!(
        findings(&explicit, "test-only").contains(&"src/main.ts".to_owned()),
        "{}",
        explicit.value
    );
    assert_eq!(
        explicit.value["summary"]["entrypoints"],
        json!({"total": 1, "byRule": {"explicit": 1}}),
        "{}",
        explicit.value
    );
    // A continuation resumes this same request, its `entries` included.
    assert!(
        explicit.next.iter().all(|(_, resume)| matches!(
            resume,
            crate::tools::ast_graph::store::Resume::Row { .. }
        )),
        "{:?}",
        explicit.next
    );
    let missing = ask(root, "issues", None, |o| {
        o.entries = vec!["src/nope.ts".into()];
    });
    assert_eq!(
        missing.class,
        ExitClass::Failed(crate::runtime::FailureKind::NotFound),
        "{}",
        missing.value
    );
}

/// `drift --baseline` diffs two snapshots by root-relative key: files
/// added, removed and edited, cycles added and resolved, imports that
/// turned dynamic, and relations added and removed.
#[test]
fn drift_diffs_two_snapshots_by_file_cycle_transition_and_relation() {
    let dir = fixture();
    let root = dir.0.as_path();
    let base = ingest_fixture(root, None);
    let base_id = base.value["id"].as_str().expect("id").to_owned();
    dir.write("app/src/b.ts", "export const b = () => 1;\n");
    dir.write(
        "app/src/main.ts",
        "import React from 'react';\nexport async function main() {\n  const { greet } = await import('./lib');\n  return greet('x');\n}\n",
    );
    dir.write("app/src/added.ts", "export const added = 1;\n");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let head = ingest_fixture(root, None);
    assert_eq!(head.class, ExitClass::Success, "{}", head.value);

    let out = ask(root, "drift", None, |o| {
        o.baseline = Some(base_id.clone());
        o.limit = Some(500);
    });
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let rows = out.value["rows"].as_array().expect("rows");
    let has = |row: Value| rows.contains(&row);
    assert!(
        has(json!({"category":"file","change":"added","file":"src/added.ts"})),
        "{}",
        out.value
    );
    assert!(
        has(json!({"category":"file","change":"changed","file":"src/b.ts"})),
        "{}",
        out.value
    );
    assert!(
        has(json!({"category":"cycle","change":"resolved","files":["src/a.ts","src/b.ts"]})),
        "{}",
        out.value
    );
    assert!(
        has(
            json!({"category":"transition","change":"staticToDynamic","from":"src/main.ts","to":"src/lib.ts"})
        ),
        "{}",
        out.value
    );
    assert!(
        has(
            json!({"category":"relation","change":"removed","from":"src/b.ts","to":"src/a.ts","edge":"imports"})
        ),
        "{}",
        out.value
    );
    // A transition is not also an added and removed relation.
    assert!(
        !rows.iter().any(|row| row["category"] == "relation"
            && row["from"] == "src/main.ts"
            && row["to"] == "src/lib.ts"),
        "{}",
        out.value
    );
    let summary = &out.value["summary"];
    assert_eq!(summary["filesAdded"], 1, "{summary}");
    assert_eq!(summary["cyclesResolved"], 1, "{summary}");
    assert_eq!(summary["staticToDynamic"], 1, "{summary}");
    assert_eq!(
        out.value["baseline"]["id"],
        base_id.as_str(),
        "{}",
        out.value
    );
    // File rows lead.
    assert_eq!(rows[0]["category"], "file", "{}", out.value);

    let same = ask(root, "drift", None, |o| {
        o.graph = head.value["id"].as_str().map(str::to_owned);
        o.baseline = head.value["id"].as_str().map(str::to_owned);
    });
    assert_eq!(same.class, ExitClass::Empty, "{}", same.value);
    let missing = ask(root, "drift", None, |_| {});
    assert_eq!(missing.class, ExitClass::InvalidInput, "{}", missing.value);
}

/// `summary.byDetector` counts the findings before `minTier` and `minScore`
/// filter the rows, so its sum is the page total plus what the filter hid;
/// the summary says so whenever they differ.
#[test]
fn by_detector_reconciles_with_the_listed_total_through_the_filter() {
    let dir = issues_fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let counted = |out: &GraphOutput| -> u64 {
        out.value["summary"]["byDetector"]
            .as_object()
            .expect("byDetector")
            .values()
            .filter_map(Value::as_u64)
            .sum()
    };
    let all = ask(dir.0.as_path(), "issues", None, |o| o.min_tier = Some(0));
    let filtered = ask(dir.0.as_path(), "issues", None, |o| o.min_tier = Some(100));
    let listed = |out: &GraphOutput| out.value["pagination"]["totalItems"].as_u64();
    let hidden = filtered.value["summary"]["hiddenBelowTier"]
        .as_u64()
        .expect("hiddenBelowTier");
    assert!(hidden > 0, "the fixture hides nothing: {}", filtered.value);
    assert_eq!(counted(&all), counted(&filtered));
    assert_eq!(
        Some(counted(&filtered)),
        listed(&filtered).map(|shown| shown + hidden),
        "{}",
        filtered.value
    );
    let notes = |out: &GraphOutput| out.value["summary"]["notes"].to_string();
    assert!(
        notes(&filtered).contains("byDetector"),
        "{}",
        filtered.value
    );
    assert!(!notes(&all).contains("byDetector"), "{}", all.value);
}

/// The `graphBuild` call a note's text offers: the JSON row after the tool name.
fn offered_build(text: &str) -> Value {
    let start = text.find("graphBuild ").expect("a graphBuild lead") + "graphBuild ".len();
    serde_json::Deserializer::from_str(&text[start..])
        .into_iter::<Value>()
        .next()
        .expect("a row")
        .expect("json")
}

/// The wider build the missing-manifest note offers rebuilds the snapshot's
/// scope above its root: the walk, `exclude` and `parserGlobs` it was built
/// with come along, their root-relative globs rebased onto the new root.
#[test]
fn the_manifest_above_lead_carries_the_snapshots_whole_scope() {
    use crate::tools::ast_graph::walk_scope::WalkScope;
    use octocode_engine::types::IgnorePolicy;
    let dir = fixture();
    dir.write("package.json", "{\"name\":\"repo\"}");
    let built = ingest(
        &IngestOptions {
            path: dir.0.join("app"),
            workspace: Some(dir.0.to_path_buf()),
            exclude: vec!["*.gen.ts".into(), "/src/skip".into()],
            walk: WalkScope {
                ignore: IgnorePolicy {
                    include_ignored: false,
                    include_hidden: true,
                },
                ..WalkScope::default()
            },
            language_globs: BTreeMap::from([("cpp".to_owned(), vec!["inc/*.h".to_owned()])]),
            ..Default::default()
        },
        &workspace_policy(dir.0.as_path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let issues = ask(dir.0.as_path(), "issues", None, |_| {});
    let note = issues.value["summary"]["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|note| note["skipped"].as_str())
        .find(|text| text.contains("holds package.json"))
        .unwrap_or_else(|| panic!("no missing-manifest note: {}", issues.value));
    assert_eq!(
        offered_build(note),
        json!({
            "path": ".",
            "exclude": ["*.gen.ts", "/app/src/skip"],
            "parserGlobs": {"cpp": ["/app/inc/*.h"]},
            "includeHidden": true,
            "includeIgnored": false,
            "defaultExcludes": true,
        }),
        "{note}"
    );
}

/// A critical file's `transitiveDependents` and its `verify` lead walk the
/// same depth: the lead lists every counted dependent, and the controls say
/// what the count leaves out.
#[test]
fn a_critical_file_lead_reaches_every_dependent_it_counts() {
    let mut files = vec![(
        "src/hub.ts".to_owned(),
        "export const hub = 1;\n".to_owned(),
    )];
    for name in ["a", "b", "c"] {
        files.push((
            format!("src/{name}.ts"),
            format!("import {{ hub }} from './hub';\nexport const {name} = hub;\n"),
        ));
    }
    // A chain of five importers above `a`: depth 6 from the hub.
    let chain = ["d", "e", "f", "g", "h"];
    for (index, name) in chain.iter().enumerate() {
        let below = if index == 0 { "a" } else { chain[index - 1] };
        files.push((
            format!("src/{name}.ts"),
            format!("import {{ {below} }} from './{below}';\nexport const {name} = {below};\n"),
        ));
    }
    let owned = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let dir = git_app(&owned);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| {
        o.detectors = vec!["critical-file".into()];
        o.min_tier = Some(0);
    });
    let finding = issues.value["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["subject"] == "src/hub.ts")
        .unwrap_or_else(|| panic!("no critical hub: {}", issues.value));
    let counted = finding["evidence"]["transitiveDependents"]
        .as_u64()
        .unwrap();
    assert_eq!(counted, 8, "{finding}");
    let lead = &finding["verify"][0]["input"]["queries"][0];
    assert_eq!(lead["operation"], "dependents", "{finding}");
    assert_eq!(lead["depth"], 6, "the deepest counted dependent: {finding}");
    let walked = ask(root, "dependents", Some("src/hub.ts"), |o| {
        o.depth = lead["depth"].as_u64().map(|depth| depth as u32);
        o.limit = Some(100);
    });
    assert_eq!(ids(&walked).len() as u64, counted, "{}", walked.value);
    // A control every finding of the detector applied is stated once, in
    // the summary; one of this finding alone stays on its row.
    let detector = finding["detector"].as_str().expect("detector");
    assert!(
        finding["controls"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(shared_controls(&issues.value, detector))
            .any(|control| control
                .as_str()
                .is_some_and(|text| text.contains("type-only"))),
        "the controls say what the count leaves out: {finding}"
    );
}

/// The summary holds what was measured on this snapshot. How findings are
/// ranked reads the same for every snapshot: the tool's description says it
/// once, and no answer repeats it.
#[test]
fn the_issues_summary_states_no_text_every_answer_would_repeat() {
    let dir = issues_fixture();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let out = ask(root, "issues", None, |o| o.limit = Some(200));
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let summary = out.value["summary"].as_object().expect("summary");
    let texts = summary
        .iter()
        .filter(|(_, value)| value.is_string())
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    assert!(texts.is_empty(), "static text in the summary: {texts:?}");
}

/// The controls `summary.notes` states once for `detector`.
fn shared_controls<'a>(answer: &'a Value, detector: &'a str) -> impl Iterator<Item = &'a Value> {
    answer["summary"]["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(move |note| note["detector"] == detector)
        .filter_map(|note| note["controls"].as_array())
        .flatten()
}

/// A detector's controls are facts of the detector: the ones every listed
/// finding of it applied go out once, in a `summary.notes` entry, and no row
/// repeats them; together they are every control each finding applied.
#[test]
fn controls_every_finding_of_a_detector_shares_are_stated_once() {
    let dir = issues_fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "issues", None, |o| {
        o.limit = Some(200);
        o.min_tier = Some(60);
    });
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let rows = out.value["rows"].as_array().expect("rows");
    let mut stated = 0;
    for row in rows {
        let detector = row["detector"].as_str().expect("detector");
        let common = shared_controls(&out.value, detector).collect::<Vec<_>>();
        stated += common.len();
        for control in row["controls"].as_array().into_iter().flatten() {
            assert!(!common.contains(&control), "{detector} repeats {control}: {row}");
        }
    }
    assert!(stated > 0, "some detector states its controls: {}", out.value);
    // A later page carries rows only: its rows are the same rows.
    let later = ask(dir.0.as_path(), "issues", None, |o| {
        o.limit = Some(1);
        o.offset = Some(1);
        o.min_tier = Some(60);
    });
    assert!(later.value.get("summary").is_none(), "{}", later.value);
    assert_eq!(later.value["rows"][0], rows[1], "{}", later.value);
}
