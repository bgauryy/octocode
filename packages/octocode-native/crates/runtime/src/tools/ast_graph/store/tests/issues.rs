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
    let cycle = out.value["results"]
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
    assert_eq!(
        findings(&out, "dev-dependency-in-production"),
        vec!["app::vitest"]
    );

    let unused = findings(&out, "unused-export");
    assert!(
        unused.contains(&"src/lib.ts".to_owned()),
        "Box is never imported: {unused:?}"
    );

    let scores = out.value["results"]
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
    assert_eq!(only.value["total"], 1);
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
    let rows = out.value["results"].as_array().cloned().unwrap_or_default();
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
    let new = out.value["results"]
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
    assert_eq!(first.value["truncated"], true, "{}", first.value);
    let second = ask(dir.0.as_path(), "issues", None, |o| {
        o.limit = Some(1);
        o.offset = Some(1);
        o.min_tier = Some(0);
    });
    assert_eq!(second.value["total"], first.value["total"]);
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
    let next = explicit.value.get("next").and_then(Value::as_str);
    assert!(
        next.is_none_or(|next| next.contains("--entry src/orphan.ts")),
        "{next:?}"
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
    let rows = out.value["results"].as_array().expect("rows");
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
