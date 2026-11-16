//! A source past the whole-file ceiling: no search row shows a value a read
//! of its lines hides, whatever the file's size. The line-for-line equality
//! with the read is held at the real limits by
//! `runtime_local::judged_sources::over_ceiling_source_rows_are_the_reads_lines`.
//! All secrets are fake.
use super::*;

/// The limits these searches and reads judge at.
const LIMITS: crate::security::SourceLimits = crate::security::SourceLimits::SMALL;
const TAIL: &str = "H7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";

/// A source of at least `bytes` bytes: a token in a literal, a URL and a
/// template at its start and at its end, around filler.
fn source(bytes: usize) -> String {
    let pieces = |name: &str| {
        format!(
            "const {name} = \"ghp_{TAIL}\";\nconst {name}Url = \"https://ci:ghp_{TAIL}@github.com/o/r\";\nconst {name}Tpl = `token ghp_{TAIL}`;\n"
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
/// its lines; returns how many rows there were and how many held a
/// placeholder.
fn rows_equal_the_read(
    path: &std::path::Path,
    fields: serde_json::Value,
    label: &str,
) -> (usize, usize) {
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
    (rows, redacted)
}

/// A token is hidden in every view of a source of any size, as a read of the
/// same lines hides it: below the whole-file ceiling and above it, where the
/// read streams.
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
            let label = format!("{name} {view}");
            let (rows, redacted) = rows_equal_the_read(&path, fields, &label);
            // A piece of the token is text a read withholds: no hit. Every
            // other search shows rows, some of them redacted.
            if view == "pieces" {
                assert_eq!(rows, 0, "{label}: a hidden piece matched");
            } else {
                assert!(rows > 0, "{label}: no rows");
                assert!(redacted > 0, "{label}: nothing redacted");
            }
        }
    }
}

/// A source past the whole-file ceiling is searched as its streamed read
/// serves it: a regex walk over a key body or a credential-named value finds
/// nothing in any view, and ordinary text and the read's placeholders count
/// as the read's own `matchString` counts them. All key material is fake.
#[test]
fn a_source_past_the_ceiling_matches_what_its_read_serves() {
    let root = tempfile::tempdir().expect("fixture");
    let path = root.path().join("big.txt");
    let filler = "ordinary filler words line\n".repeat(LIMITS.whole_source / 20);
    let body = "aMD5auQx7Tq2Lw9Pz4Vb8Nc3Kd6Hf1Jg5Rs0Ym2Xe7Wu4Ai9Oo3Pl8Ki1Uj6Yh5Tg";
    let text = format!(
        "{filler}-----BEGIN RSA PRIVATE KEY-----\n{body}\n{body}\n-----END RSA PRIVATE KEY-----\nhello world\naws_secret_access_key = \"Hx8vQ2mLp9Zr4Tn7Wk3BdAa1Bb2Cc3Dd4Ee5Ff6G\"  # hello\n{filler}"
    );
    assert!(text.len() > LIMITS.whole_source, "past the ceiling");
    fs::write(&path, &text).expect("fixture");
    let (policy, _) = policy_for(root.path());
    let security = ContentSecurity::with_limits(LIMITS);
    let count = |fields: serde_json::Value| {
        let mut fields = fields;
        fields["path"] = path.to_string_lossy().into_owned().into();
        let found = search_in(&ls_query(fields.clone(), None), &policy, &security, None)
            .expect("search")
            .to_row()
            .expect("row");
        let files = found["files"].as_array().map_or(0, Vec::len);
        let count = found["matchCount"]
            .as_u64()
            .or_else(|| found["matchedLineCount"].as_u64())
            .unwrap_or(0);
        assert_eq!(files > 0, count > 0, "{fields}: {found}");
        assert!(!found.to_string().contains("x7Tq2Lw9"), "{fields}: {found}");
        count
    };
    let read = |pattern: &str| {
        let query: octocode_contract_types::LocalFetchQuery = serde_json::from_value(
            serde_json::json!({"path": path, "matchString": [pattern], "regexEngine": "rust"}),
        )
        .expect("localFetch row");
        crate::tools::local_fetch::execute_local_fetch(
            &query,
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
        )
        .selected_match_count
        .unwrap_or(0) as u64
    };
    let views = [
        "content",
        "matchOnly",
        "files",
        "countLines",
        "countMatches",
    ];
    for pattern in [
        "^aMD5au[A-M]",
        "^aMD5au[N-Z]",
        "Hx8v[A-M]",
        "Hx8v[N-Z]",
        "x7Tq2Lw9",
    ] {
        assert_eq!(read(pattern), 0, "{pattern}: the read's own scan");
        for view in views {
            let fields =
                serde_json::json!({"matchString":[pattern],"regexEngine":"rust","view":view});
            assert_eq!(count(fields), 0, "{pattern} {view}");
        }
    }
    for pattern in ["hello", "\\[REDACTED-"] {
        let expected = read(pattern);
        assert!(expected > 0, "{pattern}");
        for view in ["content", "countLines", "countMatches"] {
            let fields =
                serde_json::json!({"matchString":[pattern],"regexEngine":"rust","view":view});
            assert_eq!(count(fields), expected, "{pattern} {view}");
        }
    }
}

/// A stopped search ends the judgment of a source past the whole-file size
/// between its lines, and reports it as interrupted rather than a failure of
/// the file.
#[test]
fn a_stopped_search_ends_the_judgment_of_a_large_source() {
    let root = tempfile::tempdir().expect("fixture directory");
    let path = root.path().join("big.ts");
    std::fs::write(
        &path,
        source(LIMITS.whole_source + LIMITS.whole_source / 10),
    )
    .expect("fixture");
    let file = std::fs::File::open(&path).expect("open");
    let judge = super::executor::judge(LIMITS);
    let stopped: &(dyn Fn() -> bool + Sync) = &|| true;
    let error = (judge.serve)(&path, &file, stopped).expect_err("a stopped judgment ends");
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    let running: &(dyn Fn() -> bool + Sync) = &|| false;
    assert!((judge.serve)(&path, &file, running).is_ok());
}
