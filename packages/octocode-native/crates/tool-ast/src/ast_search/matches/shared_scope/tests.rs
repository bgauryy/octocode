//! Rules asked of one directory walk it once and answer as each does alone
//! (perf P14); an edit the walk could have seen makes the next scan walk.
use super::WALKED;
use octocode_runtime_base::test_support::Fixture;
use serde_json::{Value, json};

fn fixture() -> Fixture {
    let root = Fixture::new();
    for (path, text) in [
        ("a.rs", "fn a() { x.push(1); x.clone(); }\n"),
        ("src/b.rs", "fn b() { y.push(2); let f = |v| v; }\n"),
        (
            "src/deep/c.rs",
            "fn c() { z.clone(); z.unwrap_or_default(); }\n",
        ),
        ("src/d.ts", "push(d);\n"),
        ("skip.log", "push\n"),
    ] {
        root.write(path, text);
    }
    root.write(".ignore", "*.log\n");
    root
}

fn rule(root: &Fixture, rule: Value) -> Value {
    json!({"operation":"match","path":root.0,"rule":rule})
}

const RULES: [&str; 4] = [
    "$X.push($Y)",
    "$X.clone()",
    "$X.unwrap_or_default()",
    "format!($$$A)",
];

/// How the directory scans under `root` walked, in order.
fn walked(root: &Fixture) -> Vec<&'static str> {
    let root = root.0.canonicalize().expect("canonical root");
    WALKED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|(walked, _)| *walked == root)
        .map(|(_, how)| *how)
        .collect()
}

/// `query`'s answer, on the calling thread's settled clock.
fn answer(root: &Fixture, query: Value) -> Value {
    super::super::super::test_rows::run(&root.0, query).expect("match")
}

/// The answer's rows: what a scan of one's own must give too.
fn rows(out: &Value) -> Value {
    json!([
        out["files"],
        out["matchCount"],
        out["filesScanned"],
        out["skippedPaths"]
    ])
}

#[test]
fn rules_asked_of_one_directory_at_once_walk_it_once_and_answer_as_alone() {
    let root = fixture();
    octocode_engine::stamp::settle_written_so_far();
    let through = octocode_engine::stamp::settled_through();
    let batch = std::thread::scope(|scope| {
        let rows = RULES
            .map(|pattern| {
                let query = rule(&root, json!({ "pattern": pattern }));
                let root = &root;
                scope.spawn(move || {
                    octocode_engine::stamp::with_settled_through(through, || answer(root, query))
                })
            })
            .into_iter()
            .collect::<Vec<_>>();
        rows.into_iter()
            .map(|row| row.join().expect("row"))
            .collect::<Vec<_>>()
    });
    // One walk; every other rule replayed it.
    let mut how = walked(&root);
    how.sort_unstable();
    assert_eq!(how, ["replay", "replay", "replay", "walk"]);
    // Each answer is the one a walk of its own gives: an exclude that
    // matches nothing is a walk of other inputs over the same files.
    for (pattern, shared) in RULES.iter().zip(&batch) {
        let mut alone = rule(&root, json!({ "pattern": pattern }));
        alone["exclude"] = json!(["nothing-here/**"]);
        let alone = answer(&root, alone);
        assert_eq!(rows(shared), rows(&alone), "{pattern}");
    }
    assert!(batch.iter().any(|out| out["matchCount"].as_u64() > Some(0)));
}

#[test]
fn an_entry_or_an_ignore_rule_the_walk_could_see_makes_the_next_scan_walk() {
    let root = fixture();
    octocode_engine::stamp::settle_written_so_far();
    let query = rule(&root, json!({"pattern":"$X.push($Y)"}));
    let first = answer(&root, query.clone());
    assert_eq!(answer(&root, query.clone()), first);
    assert_eq!(walked(&root), ["walk", "replay"]);
    // A file added in a walked directory.
    root.write("src/deep/e.rs", "fn e() { w.push(3); }\n");
    octocode_engine::stamp::settle_written_so_far();
    let added = answer(&root, query.clone());
    assert!(added.to_string().contains("e.rs"), "{added}");
    // An ignore file's rules.
    root.write(".ignore", "*.log\nsrc/deep/\n");
    octocode_engine::stamp::settle_written_so_far();
    let ignored = answer(&root, query.clone());
    assert!(!ignored["files"].to_string().contains("e.rs"), "{ignored}");
    assert_eq!(walked(&root), ["walk", "replay", "walk", "walk"]);
}
