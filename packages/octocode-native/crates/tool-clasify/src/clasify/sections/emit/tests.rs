//! The line reads a sections batch offers for its yes sections.
use super::*;
use octocode_test_support::json_fixture;

/// A ready resource whose sections at `lines` answered `noul`.
fn resource(source: Value, read: Value, sections: &[((u64, u64), f64)]) -> Value {
    let sections = sections
        .iter()
        .map(|&((start, end), noul)| {
            json!({"target":{"lines":{"startLine":start,"endLine":end}},
                "answers":{"q":{"status":"answered","answer":{"noul":noul}}}})
        })
        .collect::<Vec<_>>();
    json!({"status":"ready","source":source,"read":read,"sections":sections,
        "coverage":{"remainingSections":0}})
}

/// Of three local resources, the one whose every section said yes still
/// gets a top-level read, as does the one with a yes among noes; the one
/// with no yes gets none.
#[test]
fn every_resource_with_a_yes_gets_a_top_level_read() {
    let source = |path: &str| json!({"tool":"localFetch","query":{"path":path}});
    let resources = json!({
        "a": resource(source("a.rs"), Value::Null, &[((1, 10), 0.9), ((11, 20), 0.8)]),
        "b": resource(source("b.rs"), Value::Null, &[((1, 10), 0.1), ((11, 20), 0.7)]),
        "c": resource(source("c.rs"), Value::Null, &[((1, 10), 0.1)]),
    });
    let reads = hit_reads(resources.as_object().expect("resources"), |_| None);
    let read: Vec<(&Value, &Value)> = reads
        .iter()
        .map(|read| {
            let row = &read["input"]["queries"][0];
            (&row["path"], &row["ranges"])
        })
        .collect();
    assert_eq!(
        read,
        [
            (&json!("a.rs"), &json!([{"startLine": 1, "endLine": 20}])),
            (&json!("b.rs"), &json!([{"startLine": 11, "endLine": 20}])),
        ]
    );
}

/// A URL capture's yes sections are read as lines of its saved file, at
/// the capture's snapshot, as a file's are.
#[test]
fn a_url_resource_reads_its_yes_sections() {
    let whole = octocode_tools_shared::source_capture::read_lead(
        octocode_runtime_base::id::ToolId::LocalFetch,
        &json_fixture(
            r#"{
                "path": "/cache/capture/url/d1/source.txt",
                "ranges": [{"startLine": 1, "endLine": 90}]
            }"#,
        ),
        Some(&"a1".repeat(32)),
    );
    let page = resource(
        json_fixture(r#"{"url": "https://example.test/guide"}"#),
        whole.clone(),
        &[((1, 30), 0.1), ((31, 60), 0.9), ((61, 90), 0.2)],
    );
    let resources = json!({"guide": page});
    let reads = hit_reads(resources.as_object().expect("resources"), |_| None);
    assert_eq!(reads.len(), 1, "{reads:?}");
    let (row, saved) = (
        &reads[0]["input"]["queries"][0],
        &whole["input"]["queries"][0],
    );
    assert_eq!(
        row["ranges"],
        json_fixture(r#"[{"startLine": 31, "endLine": 60}]"#)
    );
    assert_eq!(
        (&row["path"], &row["snapshot"]),
        (&saved["path"], &saved["snapshot"])
    );
}

/// A resource's own `read` and a top-level read of its yes sections that
/// name the same lines are one read: the page states it once. A yes on a
/// narrower span still gets its own, narrower read.
#[test]
fn a_read_the_resource_already_states_is_not_repeated() {
    let whole = |query: &str| {
        octocode_tools_shared::source_capture::read_lead(
            octocode_runtime_base::id::ToolId::LocalFetch,
            &json_fixture(query),
            Some(&"a1".repeat(32)),
        )
    };
    let capture = whole(
        r#"{
            "path": "/cache/capture/url/d1/source.txt",
            "ranges": [{"startLine": 1, "endLine": 90}]
        }"#,
    );
    let file = whole(r#"{"path": "/w/a.rs", "ranges": [{"startLine": 5, "endLine": 40}]}"#);
    let ranged = json_fixture(
        r#"{
            "tool": "localFetch",
            "query": {"path": "a.rs", "ranges": [{"startLine": 5, "endLine": 40}]}
        }"#,
    );
    let url = json_fixture(r#"{"url": "https://example.test/guide"}"#);
    let resources = json!({
        "everyLine": resource(url.clone(), capture.clone(), &[((1, 45), 0.9), ((46, 90), 0.8)]),
        "ranged": resource(ranged.clone(), file.clone(), &[((5, 40), 0.9)]),
        "narrower": resource(ranged, file.clone(), &[((5, 20), 0.9), ((21, 40), 0.1)]),
    });
    let resources = resources.as_object().expect("resources");
    let reads = hit_reads(resources, |_| None);
    let stated: Vec<&Value> = resources
        .values()
        .map(|resource| &resource["read"]["input"]["queries"][0]["ranges"])
        .collect();
    let offered: Vec<&Value> = reads
        .iter()
        .map(|read| &read["input"]["queries"][0]["ranges"])
        .collect();
    assert_eq!(
        offered,
        [&json_fixture(r#"[{"startLine": 5, "endLine": 20}]"#)],
        "only the read no resource row states: {reads:?}"
    );
    assert!(
        offered.iter().all(|ranges| !stated.contains(ranges)),
        "{reads:?}"
    );
}
