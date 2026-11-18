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
