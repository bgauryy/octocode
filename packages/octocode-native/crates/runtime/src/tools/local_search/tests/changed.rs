//! A matched file written while a call reads it. A fresh call is its own
//! scan: one file that changes under it withholds that file's values and
//! says so, and the rest of the page stays verified. A continuation is cut
//! from a stored scan, so a changed file restarts its snapshot.
use super::*;
use crate::tools::local_search::executor;
use crate::tools::local_search::verify::UNVERIFIED_PLACEHOLDER;
use octocode_contract_types::ErrorCode;

const CHANGED_DISCLOSURE: &str = "changed during the call";

/// The value of every shown row, by `path`.
fn shown_values(result: &LocalSearchResult) -> Vec<(String, Vec<String>)> {
    result
        .files
        .iter()
        .map(|file| {
            let values = file
                .matches
                .iter()
                .flatten()
                .map(|row| row.value.clone())
                .collect();
            (file.path.clone(), values)
        })
        .collect()
}

#[test]
fn a_fresh_page_withholds_only_the_file_written_after_its_scan() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.ts"), "const needle = 1;\n").expect("fixture");
    fs::write(root.path().join("b.ts"), "const needle = 2;\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path":root.path(),"matchString":["needle"],"sort":{"by":"path","order":"asc"}}),
        None,
    );
    let scan = executor::scan_query(&request, &policy, &NeverCancel, (None, None)).expect("scan");
    assert!(
        scan.digests.is_none(),
        "a call without a snapshot scans afresh"
    );
    assert!(
        scan.parsed
            .files
            .iter()
            .flat_map(|file| &file.matches)
            .all(|hit| hit.deferred.is_some()),
        "the page rebuilds each value from its source"
    );
    // Appended between the scan and the page's reads, as a log being written.
    fs::write(
        root.path().join("a.ts"),
        "const needle = 1;\n// appended while the call runs\n",
    )
    .expect("append");
    let alternates = crate::tools::local_search::probes::Alternates::new(&request, &scan);
    let page = executor::page_of(
        &request,
        &policy,
        &security,
        &NeverCancel,
        scan,
        None,
        &alternates,
    )
    .expect("one file changing under a fresh call does not fail the call");
    assert_eq!(
        shown_values(&page),
        vec![
            ("a.ts".to_owned(), vec![UNVERIFIED_PLACEHOLDER.to_owned()]),
            ("b.ts".to_owned(), vec!["const needle = 2;".to_owned()]),
        ],
        "only the changed file's values are withheld"
    );
    assert!(
        page.warnings
            .iter()
            .any(|warning| warning.contains(CHANGED_DISCLOSURE)),
        "the page discloses the withheld values: {:?}",
        page.warnings
    );
}

#[test]
fn an_unchanged_fresh_page_discloses_nothing() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.ts"), "const needle = 1;\n").expect("fixture");
    let page = search_root(root.path(), serde_json::json!({"matchString":["needle"]}));
    assert_eq!(
        shown_values(&page),
        vec![("a.ts".to_owned(), vec!["const needle = 1;".to_owned()])]
    );
    assert!(
        !page
            .warnings
            .iter()
            .any(|warning| warning.contains(CHANGED_DISCLOSURE)),
        "{:?}",
        page.warnings
    );
}

#[test]
fn a_continuation_whose_stored_file_changed_still_restarts() {
    let root = tempfile::tempdir().expect("fixture");
    let names = (0..30).map(|n| format!("m{n:02}.ts")).collect::<Vec<_>>();
    for name in &names {
        let body = (0..12)
            .map(|n| format!("export const needle_{n} = {n};\n"))
            .collect::<String>();
        fs::write(root.path().join(name), body).expect("fixture");
    }
    settle();
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path":root.path(),"matchString":["needle"],"sort":{"by":"path","order":"asc"}}),
        None,
    );
    let first = search_in(&request, &policy, &security, Some(1500)).expect("page 1");
    let snapshot = first
        .source_snapshot
        .expect("a paged answer stores its scan");
    let continued = ls_query(
        serde_json::json!({"snapshot":snapshot,"page":2}),
        Some(&request),
    );
    let error = stored_page_after(&continued, &policy, &security, Some(1500), || {
        for name in &names {
            let path = root.path().join(name);
            let mut body = fs::read_to_string(&path).expect("source");
            body.push_str("// appended after the stored scan\n");
            fs::write(path, body).expect("append");
        }
    })
    .expect_err("a stored scan never mixes evidence");
    assert_eq!(error.code, ErrorCode::StaleSnapshot, "{error:?}");
}
