use super::scan::DIRECTORY_SCANS;
use crate::tools::test_support::Fixture;
use serde_json::json;

/// Identical match requests running at once are independent: each walks,
/// reads and parses the directory itself, and both answer alike. No scan or
/// result is shared across requests.
#[test]
fn identical_concurrent_requests_each_perform_their_own_scan() {
    let root = Fixture::new();
    for index in 0..60 {
        root.write(&format!("f{index:02}.rs"), "fn f() { x.unwrap(); }\n");
    }
    let query = json!({"operation":"match","path":root.0,"rule":{"pattern":"$X.unwrap()"},"view":"countMatches"});
    let canonical = root.0.canonicalize().expect("canonical");
    let scans = || {
        DIRECTORY_SCANS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|scanned| **scanned == canonical)
            .count()
    };
    let before = scans();
    let gate = std::sync::Barrier::new(2);
    let outs = std::thread::scope(|scope| {
        let requests = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    gate.wait();
                    super::super::test_rows::run(&root.0, query.clone()).expect("match")
                })
            })
            .collect::<Vec<_>>();
        requests
            .into_iter()
            .map(|request| request.join().expect("request"))
            .collect::<Vec<_>>()
    });
    assert_eq!(outs[0], outs[1]);
    assert_eq!(outs[0]["matchCount"], 60, "{}", outs[0]);
    assert_eq!(scans() - before, 2, "one scan per request");
}
