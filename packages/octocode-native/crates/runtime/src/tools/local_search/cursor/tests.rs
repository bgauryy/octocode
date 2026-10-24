use super::*;
use octocode_engine::types::{SearchedSource, TextSearchFile, TextSearchMatch, TextSearchStats};

fn scan() -> (Vec<TextSearchFile>, TextSearchStats) {
    let hit = |line: u32, value: &str| TextSearchMatch {
        deferred: None,
        line,
        last_line: None,
        column: 3,
        byte_offset: 3,
        value: value.into(),
        value_digest: [line as u8; 32],
        count: None,
        rank: Some(1),
        original_chars: None,
        original_bytes: None,
    };
    let files = vec![
        TextSearchFile {
            path: "src/a.rs".into(),
            match_count: 2,
            matches: vec![hit(1, "fn needle()"), hit(9, "needle();")],
            source: Some(SearchedSource {
                size: 120,
                modified: Some(std::time::SystemTime::UNIX_EPOCH),
                digest: [1; 32],
            }),
        },
        TextSearchFile {
            path: "src/b.rs".into(),
            match_count: 1,
            matches: vec![hit(4, "needle")],
            source: None,
        },
    ];
    let stats = TextSearchStats {
        match_count: Some(3),
        matched_lines: Some(3),
        files_matched: Some(2),
        files_searched: Some(10),
        ..TextSearchStats::default()
    };
    (files, stats)
}

/// The snapshot binds every collected field of every row, each file's
/// source digest and the totals, and nothing else: a source's size and
/// time alone (its bytes unchanged) keep it. It is a v8 snapshot a
/// later call accepts as well formed.
#[test]
fn the_snapshot_binds_every_row_field_and_no_stamp() {
    let (files, stats) = scan();
    let base = fingerprint("key", &files, &stats);
    assert!(base.starts_with("lexical-live-v8:"), "{base}");
    assert_eq!(fingerprint("key", &files, &stats), base);
    let snapshot = serde_json::json!({"path": ".", "matchString": ["x"], "snapshot": base});
    let query: LocalSearchQuery = serde_json::from_value(snapshot).expect("query");
    assert!(malformed_snapshot(&query).is_none());

    type Edit = fn(&mut Vec<TextSearchFile>, &mut TextSearchStats);
    let bound: [(&str, Edit); 18] = [
        ("path", |f, _| f[0].path.push('x')),
        ("matchCount", |f, _| f[0].match_count += 1),
        ("line", |f, _| f[0].matches[0].line += 1),
        ("column", |f, _| f[0].matches[0].column += 1),
        ("byteOffset", |f, _| f[0].matches[0].byte_offset += 1),
        ("value", |f, _| f[0].matches[0].value.push('x')),
        ("valueDigest", |f, _| f[0].matches[0].value_digest[0] ^= 1),
        ("rank", |f, _| f[0].matches[0].rank = Some(3)),
        ("originalChars", |f, _| {
            f[0].matches[0].original_chars = Some(9);
        }),
        ("originalBytes", |f, _| {
            f[0].matches[0].original_bytes = Some(9);
        }),
        ("count", |f, _| f[0].matches[0].count = Some(1)),
        ("sourceDigest", |f, _| {
            if let Some(source) = &mut f[0].source {
                source.digest[0] ^= 1;
            }
        }),
        ("source", |f, _| {
            f[1].source = Some(SearchedSource {
                size: 0,
                modified: None,
                digest: [0; 32],
            });
        }),
        ("rows", |f, _| {
            let row = f[0].matches.remove(1);
            f[1].matches.push(row);
        }),
        ("matchTotal", |_, s| s.match_count = Some(4)),
        ("filesScanned", |_, s| s.files_searched = Some(11)),
        ("capReason", |_, s| {
            s.cap_reasons = vec![octocode_engine::types::CapReason::MaxCollectedFiles];
        }),
        ("firstError", |_, s| s.first_error = Some("denied".into())),
    ];
    for (field, edit) in bound {
        let (mut files, mut stats) = scan();
        edit(&mut files, &mut stats);
        assert_ne!(fingerprint("key", &files, &stats), base, "{field}");
    }
    assert_ne!(fingerprint("key2", &files, &stats), base, "query key");
    // Text boundaries cannot shift between fields.
    let (mut shifted, _) = scan();
    shifted[0].path = "src/a.r".into();
    shifted[0].matches[0].value = "sfn needle()".into();
    assert_ne!(fingerprint("key", &shifted, &stats), base);

    let (mut stamped, _) = scan();
    if let Some(source) = &mut stamped[0].source {
        source.size = 999;
        source.modified = None;
    }
    assert_eq!(fingerprint("key", &stamped, &stats), base, "stamps only");
    // An error count of zero is no error, as before.
    let mut zero = stats.clone();
    zero.error_count = Some(0);
    assert_eq!(fingerprint("key", &files, &zero), base);
}
