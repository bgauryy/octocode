//! A source past the whole-file ceiling: no search row shows a value a read
//! of its lines hides, whatever the file's size. The line-for-line equality
//! with the read is held at the real limits by
//! `runtime_local::judged_sources::over_ceiling_source_rows_are_the_reads_lines`.
//! All secrets are fake.
use super::*;

/// The limits these searches and reads judge at.
const LIMITS: crate::security::SourceLimits = crate::security::SourceLimits::SMALL;
const TAIL: &str = "H7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";

/// A source of at least `bytes` bytes: a token written in pieces (a `+`
/// join over two lines, an array, a template) at its start and at its end,
/// around filler.
fn source(bytes: usize) -> String {
    let pieces = |name: &str| {
        format!(
            "const {name} = \"ghp_\" +\n  \"{TAIL}\";\nconst {name}Parts = [\"ghp_\", \"{TAIL}\"].join(\"\");\nconst {name}Tpl = `${{\"ghp_\"}}${{\"{TAIL}\"}}`;\n"
        )
    };
    let mut text = pieces("early");
    let mut line = 0;
    while text.len() < bytes {
        text.push_str(&format!("export const filler{line} = {line}; // plain\n"));
        line += 1;
    }
    text.push_str(&pieces("late"));
    text
}

/// Lines `first..=last` of `path` as localFetch serves them.
fn fetched(
    path: &std::path::Path,
    (first, last): (u64, u64),
    policy: &PathPolicy,
    security: &ContentSecurity,
) -> Vec<String> {
    let query: octocode_contract_types::LocalFetchQuery = serde_json::from_value(
        serde_json::json!({"path": path, "ranges": [{"startLine": first, "endLine": last}]}),
    )
    .expect("localFetch row");
    let read = crate::tools::local_fetch::execute_local_fetch(
        &query,
        policy,
        security,
        &NeverCancel,
        None,
        None,
    );
    let content = read.content.clone().unwrap_or_else(|| panic!("{read:?}"));
    let content = content.strip_suffix('\n').unwrap_or(&content);
    content.split('\n').map(str::to_owned).collect()
}

/// Every row of `fields` searched in `path`, checked against the read of
/// its lines; returns how many rows held a placeholder.
fn rows_equal_the_read(path: &std::path::Path, fields: serde_json::Value, label: &str) -> usize {
    let root = path.parent().expect("root");
    let (policy, _) = policy_for(root);
    let security = ContentSecurity::with_limits(LIMITS);
    let mut fields = fields;
    fields["path"] = path.to_string_lossy().into_owned().into();
    let span = fields["view"] == "matchOnly";
    let result = search_in(&ls_query(fields, None), &policy, &security, None).expect("search");
    let text = values(&result);
    assert!(!text.contains(TAIL), "{label}: a piece in clear: {text}");
    let mut redacted = 0;
    let mut rows = 0;
    for hit in result
        .files
        .iter()
        .flat_map(|file| file.matches.iter().flatten())
    {
        rows += 1;
        let value = hit.value.as_str();
        redacted += usize::from(value.contains("[REDACTED"));
        let line = u64::from(hit.line);
        if span {
            // A span is a cut of its line as the read serves it.
            let served = fetched(path, (line, line), &policy, &security).join("\n");
            assert!(
                served.contains(value) || value.contains("[REDACTED"),
                "{label} {line}: {value:?} is no cut of {served:?}"
            );
            continue;
        }
    }
    assert!(rows > 0, "{label}: no rows: {text}");
    redacted
}

/// A token split over lines, an array or a template is hidden in every view
/// of a source of any size, as a read of the same lines hides it: below the
/// whole-file ceiling and above it, where the read streams.
#[test]
fn a_row_of_a_source_past_the_whole_file_ceiling_is_the_reads_line() {
    let root = tempfile::tempdir().expect("fixture");
    for (name, bytes) in [
        ("under.ts", LIMITS.whole_source / 2),
        ("over.ts", LIMITS.whole_source + LIMITS.whole_source / 50),
        ("far.ts", 3 * LIMITS.whole_source),
    ] {
        let path = root.path().join(name);
        fs::write(&path, source(bytes)).expect("fixture");
        for (view, fields) in [
            (
                "content",
                serde_json::json!({"matchString":["early|late"],"regexEngine":"rust"}),
            ),
            (
                "context",
                serde_json::json!({"matchString":["early|late"],"regexEngine":"rust","contextLines":{"before":1,"after":2}}),
            ),
            (
                "matchOnly",
                serde_json::json!({"matchString":["[\"`][^\"`]*[\"`]"],"regexEngine":"rust","view":"matchOnly","limit":40}),
            ),
            (
                "pieces",
                serde_json::json!({"matchString":[TAIL.get(..12).expect("piece")],"view":"matchOnly"}),
            ),
        ] {
            let redacted = rows_equal_the_read(&path, fields, &format!("{name} {view}"));
            assert!(redacted > 0, "{name} {view}: nothing redacted");
        }
    }
}
