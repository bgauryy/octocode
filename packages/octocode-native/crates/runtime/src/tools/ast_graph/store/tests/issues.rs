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

    assert_eq!(
        findings(&out, "cycle"),
        vec!["app/src/a.ts"],
        "{}",
        out.value
    );
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
        unreachable.contains(&"app/src/orphan.ts".to_owned()),
        "{unreachable:?}"
    );
    assert!(
        !unreachable.contains(&"app/src/client.gen.ts".to_owned()),
        "generated files are never subjects"
    );
    assert!(!unreachable.contains(&"app/src/main.ts".to_owned()));

    assert_eq!(findings(&out, "test-only"), vec!["app/src/fmt.ts"]);
    assert_eq!(
        findings(&out, "undeclared-dependency"),
        vec!["app::left-pad"]
    );

    let unused = findings(&out, "unused-export");
    assert!(
        unused.contains(&"app/src/lib.ts".to_owned()),
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
    assert_eq!(only.value["pagination"]["total"], 1);
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
    assert!(new.contains(&"app/src/orphan2.ts"), "{new:?}");
}

/// `resolved` means gone from the head snapshot, never hidden by the view:
/// a finding `minTier` hides is still present. Against itself a snapshot
/// resolves nothing at any view, and its hidden findings stay out of
/// `resolvedFindings`.
#[test]
fn a_baseline_never_resolves_a_finding_the_view_hides() {
    let dir = git_app(&[
        ("package.json", r#"{"name":"app","main":"src/main.ts"}"#),
        ("src/main.ts", "export const worker = 'src/worker.ts';\n"),
        ("src/worker.ts", "export const work = () => 1;\n"),
    ]);
    let root = dir.0.as_path();
    let built = ingest_fixture(root, None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let id = built.value["id"].as_str().expect("id").to_owned();
    for min_tier in [None, Some(60), Some(100)] {
        let out = ask(root, "issues", None, |o| {
            o.baseline = Some(id.clone());
            o.min_tier = min_tier;
        });
        let baseline = &out.value["baseline"];
        assert_eq!(baseline["resolved"], 0, "{min_tier:?}: {}", out.value);
        assert_eq!(baseline["resolvedFindings"], json!([]), "{baseline}");
        assert_eq!(baseline["new"], 0, "{baseline}");
        let listed = out.value["pagination"]["total"].as_u64().unwrap_or(0);
        let hidden = out.value["summary"]["hiddenBelowTier"]
            .as_u64()
            .unwrap_or(0);
        assert_eq!(
            baseline["existing"].as_u64(),
            Some(listed + hidden),
            "every head finding, listed or hidden, is existing: {}",
            out.value
        );
        if min_tier.is_none() {
            assert!(
                hidden > 0,
                "the default view hides the mentioned worker: {}",
                out.value
            );
        }
    }
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
        second.value["pagination"]["total"],
        first.value["pagination"]["total"]
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
    assert!(findings(&inferred, "unreachable-file").contains(&"app/src/orphan.ts".to_owned()));
    let explicit = ask(root, "issues", None, |o| {
        o.detectors = vec!["unreachable-file".into(), "test-only".into()];
        o.entries = vec!["src/orphan.ts".into()];
        o.limit = Some(200);
        o.min_tier = Some(0);
    });
    let unreachable = findings(&explicit, "unreachable-file");
    assert!(
        !unreachable.contains(&"app/src/orphan.ts".to_owned()),
        "{}",
        explicit.value
    );
    // main.ts is no entry now: only its test reaches it.
    assert!(
        findings(&explicit, "test-only").contains(&"app/src/main.ts".to_owned()),
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
        has(json!({"category":"file","change":"added","path":"app/src/added.ts"})),
        "{}",
        out.value
    );
    assert!(
        has(json!({"category":"file","change":"changed","path":"app/src/b.ts"})),
        "{}",
        out.value
    );
    assert!(
        has(
            json!({"category":"cycle","change":"resolved","files":["app/src/a.ts","app/src/b.ts"]})
        ),
        "{}",
        out.value
    );
    assert!(
        has(
            json!({"category":"transition","change":"staticToDynamic","from":"app/src/main.ts","to":"app/src/lib.ts"})
        ),
        "{}",
        out.value
    );
    assert!(
        has(
            json!({"category":"relation","change":"removed","from":"app/src/b.ts","to":"app/src/a.ts","edge":"imports"})
        ),
        "{}",
        out.value
    );
    // A transition is not also an added and removed relation.
    assert!(
        !rows.iter().any(|row| row["category"] == "relation"
            && row["from"] == "app/src/main.ts"
            && row["to"] == "app/src/lib.ts"),
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
    let listed = |out: &GraphOutput| out.value["pagination"]["total"].as_u64();
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
    // The hidden count is stated once: `hiddenBelowTier`, never again on a note.
    let restated = filtered.value["summary"]["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|note| note.get("hidden").is_some())
        .count();
    assert_eq!(restated, 0, "{}", filtered.value["summary"]);
}

/// The wider build the missing-manifest lead offers rebuilds the snapshot's
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
    let lead = issues.value["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["tool"] == "graphBuild")
        .unwrap_or_else(|| panic!("no missing-manifest lead: {}", issues.value));
    // The workspace root is the default `path`, so the call leaves it out.
    assert_eq!(
        lead["input"]["queries"][0],
        json!({
            "exclude": ["*.gen.ts", "/app/src/skip"],
            "parserGlobs": {"cpp": ["/app/inc/*.h"]},
            "includeHidden": true,
            "includeIgnored": false,
            "defaultExcludes": true,
        }),
        "{lead}"
    );
}

/// A critical file's `transitiveDependents` and its `farthestDepth` are one
/// walk: `dependents` to that depth lists every counted dependent, and the
/// controls say what the count leaves out.
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
        .find(|row| row["subject"] == "app/src/hub.ts")
        .unwrap_or_else(|| panic!("no critical hub: {}", issues.value));
    let counted = finding["evidence"]["transitiveDependents"]
        .as_u64()
        .unwrap();
    assert_eq!(counted, 8, "{finding}");
    // A rank is a share, not a measurement: 4 significant digits.
    let rank = finding["evidence"]["pageRank"].as_f64().expect("pageRank");
    assert_eq!(
        format!("{rank:.3e}").parse::<f64>().ok(),
        Some(rank),
        "{finding}"
    );
    let depth = &finding["evidence"]["farthestDepth"];
    assert_eq!(*depth, 6, "the deepest counted dependent: {finding}");
    assert!(finding.get("verify").is_none(), "{finding}");
    let walked = ask(root, "dependents", Some("src/hub.ts"), |o| {
        o.depth = depth.as_u64().map(|depth| depth as u32);
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

/// What `summary.notes` states every listed row of `detector` shares.
fn shared<'a>(answer: &'a Value, detector: &str) -> &'a Value {
    answer["summary"]["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|note| note["detector"] == detector && note.get("shared").is_some())
        .map_or(&Value::Null, |note| &note["shared"])
}

/// The controls `summary.notes` states once for `detector`.
fn shared_controls<'a>(answer: &'a Value, detector: &str) -> impl Iterator<Item = &'a Value> {
    shared(answer, detector)["controls"]
        .as_array()
        .into_iter()
        .flatten()
}

/// A detector's controls are facts of the detector: the ones every listed
/// finding of it applied go out once, in a `summary.notes` entry
/// `{detector, shared}`, and no row repeats them; together they are every control each finding
/// applied. Any other field every row of the detector states alike goes
/// there too, and no row repeats it.
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
            assert!(
                !common.contains(&control),
                "{detector} repeats {control}: {row}"
            );
        }
        let constants = shared(&out.value, detector);
        for (key, value) in constants.as_object().into_iter().flatten() {
            if key == "evidence" {
                for inner in value.as_object().into_iter().flatten().map(|(key, _)| key) {
                    assert!(row["evidence"].get(inner).is_none(), "{inner}: {row}");
                }
            } else if key != "controls" {
                assert!(row.get(key).is_none(), "{detector} repeats {key}: {row}");
            }
        }
    }
    let stated_scalars = rows.iter().any(|row| {
        shared(&out.value, row["detector"].as_str().unwrap_or_default())["confidence"].is_number()
    });
    assert!(
        stated_scalars,
        "a detector states its confidence: {}",
        out.value
    );
    assert!(
        stated > 0,
        "some detector states its controls: {}",
        out.value
    );
    // A later page carries rows only: its rows are the same rows.
    let later = ask(dir.0.as_path(), "issues", None, |o| {
        o.limit = Some(1);
        o.offset = Some(1);
        o.min_tier = Some(60);
    });
    assert!(later.value.get("summary").is_none(), "{}", later.value);
    assert_eq!(later.value["rows"][0], rows[1], "{}", later.value);
}

/// Every file an answer names is a path from the workspace root, as rows'
/// `path` is, so it goes straight to localFetch or lspSearch: hubs `id`,
/// an issue's `subject`, a cycle's `members`, `witness` and `suggestedCuts`.
/// A symbol id stays the snapshot's id, its file named by `path` beside it.
#[test]
fn answers_name_files_from_the_workspace_and_keep_symbol_ids() {
    let dir = fixture();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let hubs = ask(root, "hubs", None, |_| {});
    let hub_ids = ids(&hubs);
    assert!(!hub_ids.is_empty(), "{}", hubs.value);
    for id in hub_ids.iter().filter(|id| !id.starts_with("pkg:")) {
        assert!(id.starts_with("app/src/"), "{id}: {}", hubs.value);
        assert!(root.join(id).is_file(), "{id}");
    }
    let issues = ask(root, "issues", None, |o| {
        o.detectors = vec!["cycle".into()];
        o.min_tier = Some(0);
    });
    let cycle = issues.value["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["detector"] == "cycle")
        .unwrap_or_else(|| panic!("cycle finding: {}", issues.value));
    assert_eq!(cycle["subject"], "app/src/a.ts", "{cycle}");
    let evidence = &cycle["evidence"];
    assert_eq!(
        evidence["members"],
        json!(["app/src/a.ts", "app/src/b.ts"]),
        "{cycle}"
    );
    for list in ["witness", "suggestedCuts"] {
        let hops = evidence[list].as_array().expect("hops");
        assert!(!hops.is_empty(), "{list}: {cycle}");
        for hop in hops {
            for end in ["from", "to"] {
                let file = hop[end].as_str().expect("file");
                assert!(file.starts_with("app/src/"), "{list}.{end}: {cycle}");
                assert!(root.join(file).is_file(), "{list}.{end}: {file}");
            }
        }
    }
    let cycles = ask(root, "cycles", None, |_| {});
    assert_eq!(
        cycles.value["rows"][0]["nodes"],
        json!(["app/src/a.ts", "app/src/b.ts"]),
        "{}",
        cycles.value
    );
    let found = ask(root, "find", Some("greet"), |o| {
        o.kind = Some("symbol".into());
    });
    assert_eq!(
        found.value["rows"][0]["id"], "src/lib.ts#greet",
        "{}",
        found.value
    );
    assert_eq!(
        found.value["rows"][0]["path"], "app/src/lib.ts",
        "{}",
        found.value
    );
    // The workspace spelling of a file reads as its snapshot key.
    let node = ask(root, "node", Some("app/src/lib.ts"), |_| {});
    assert_eq!(node.value["node"]["id"], "app/src/lib.ts", "{}", node.value);
    // A mention names its file the same way, with the line after it.
    let named = git_app(&[
        ("package.json", r#"{"name":"app","main":"src/main.ts"}"#),
        (
            "src/main.ts",
            "import { names } from './catalog';\nexport const main = () => names.map((name) => import(`./checks/${name}.js`));\n",
        ),
        ("src/catalog.ts", "export const names = ['late-check'];\n"),
        ("src/checks/late-check.ts", "export const run = () => 1;\n"),
    ]);
    let root = named.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| {
        o.detectors = vec!["unreachable-file".into()];
        o.min_tier = Some(0);
    });
    let row = issues.value["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["subject"] == "app/src/checks/late-check.ts")
        .unwrap_or_else(|| panic!("mentioned file: {}", issues.value));
    let mention = row["evidence"]["mentionedIn"][0].as_str().expect("mention");
    assert_eq!(mention, "app/src/catalog.ts:1", "{row}");
    let (file, _) = mention.rsplit_once(':').expect("path:line");
    assert!(root.join(file).is_file(), "{mention}");
}
