//! Every private-key format through `localFetch`: whole, from inside the
//! block, across a page boundary, and by a body line's own text. The scan is
//! `Safe`, so only the block guard can have redacted them.

use super::*;

use crate::security::key_formats as formats;

/// Every page of `query` under a page `budget`, joined.
fn all_pages(query: LocalFetchQuery, paths: &Paths, budget: Option<usize>) -> String {
    all_pages_with(query, paths, budget, &Safe)
}

/// [`all_pages`] under the content scan `scan`.
fn all_pages_with(
    mut query: LocalFetchQuery,
    paths: &Paths,
    budget: Option<usize>,
    scan: &impl ContentScan,
) -> String {
    let mut out = String::new();
    for _ in 0..200 {
        let result = execute_local_fetch(&query, paths, scan, &NeverCancel, budget, None);
        out.push_str(result.content.as_deref().unwrap_or_default());
        match result.next.and_then(|next| next.continue_window) {
            Some(next) => query = next.query,
            None => return out,
        }
    }
    panic!("pages must end");
}

fn assert_hidden(text: &str, format: &formats::KeyFormat, how: &str) {
    for secret in &format.secrets {
        assert!(
            !text.contains(secret.as_str()),
            "{} {how}: {text}",
            format.name
        );
    }
}

#[test]
fn no_read_of_any_key_format_shows_a_body_line() {
    for (index, format) in formats::formats().iter().enumerate() {
        let fixture = Fixture::new();
        let path = fixture.write(&format!("notes{index}.txt"), format.text.clone());
        let paths = Paths(fixture.0.clone());
        let whole = all_pages(qj(&path, serde_json::json!({})), &paths, None);
        assert_hidden(&whole, format, "whole");
        assert!(whole.contains("tail line"), "{}: {whole}", format.name);
        let total = format.text.lines().count();
        for first in 1..=total {
            let range = serde_json::json!({"ranges":[{"startLine":first,"endLine":first + 1}]});
            assert_hidden(&all_pages(qj(&path, range), &paths, None), format, "range");
        }
        // A page boundary falls inside the block.
        for budget in [120, 300] {
            let paged = all_pages(qj(&path, serde_json::json!({})), &paths, Some(budget));
            assert_hidden(&paged, format, "paged");
            assert!(paged.contains("tail line"), "{}: {paged}", format.name);
        }
        for secret in &format.secrets {
            let probe =
                serde_json::json!({"matchString":[secret],"contextLines":{"before":1,"after":1}});
            let result = fetch(&qj(&path, probe), &paths);
            assert_hidden(
                result.content.as_deref().unwrap_or_default(),
                format,
                "matchString",
            );
        }
    }
}

/// A stream past the whole-file ceiling: a window starting after the marker,
/// and one found by a body line's text, both served from the block state the
/// pass carried over the source.
#[test]
fn no_streamed_read_shows_a_body_line() {
    for name in [
        "rfc4716 encrypted",
        "ppk v3",
        "tab after begin",
        "marker past 256 bytes",
        "end then begin joined",
        "end then begin with no later end",
        "comment in a body",
        "prose before a body with no end",
    ] {
        let all = formats::formats();
        let format = all
            .iter()
            .find(|format| format.name == name)
            .expect("format");
        let fixture = Fixture::new();
        // Past the whole-file ceiling of the scan the reads run under.
        let lead = SmallSafe.limits().whole_source / "outside\n".len() + 1_000;
        let filler = "outside\n".repeat(lead);
        let path = fixture.write("large.log", format!("{filler}{}", format.text));
        let paths = Paths(fixture.0.clone());
        let total = format.text.lines().count();
        let whole = serde_json::json!({"ranges":[{"startLine":lead + 1,"endLine":lead + total}]});
        let shown = all_pages_with(qj(&path, whole), &paths, None, &SmallSafe);
        assert_hidden(&shown, format, "stream whole");
        assert!(shown.contains("tail line"), "{name}: {shown}");
        let at = format
            .text
            .lines()
            .position(|line| line.contains(&format.secrets[1]))
            .expect("a body line");
        let inside =
            serde_json::json!({"ranges":[{"startLine":lead + at + 1,"endLine":lead + total}]});
        assert_hidden(
            &all_pages_with(qj(&path, inside), &paths, None, &SmallSafe),
            format,
            "stream inside",
        );
        let probe = serde_json::json!({"matchString":[format.secrets[1]],"contextLines":{"before":2,"after":2}});
        let result = fetch_with(&qj(&path, probe), &paths, &SmallSafe);
        assert_hidden(
            result.content.as_deref().unwrap_or_default(),
            format,
            "stream matchString",
        );
    }
}

#[test]
fn a_near_miss_marker_leaves_the_lines_after_it_readable() {
    let run = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIzNDU2Nzg5QUJDREVGR0hJSktM";
    for miss in formats::NOT_MARKERS {
        let fixture = Fixture::new();
        let text = format!("{miss}\n{run}\nafter\n");
        let path = fixture.write("notes.txt", &text);
        let result = fetch(&qj(&path, serde_json::json!({})), &Paths(fixture.0.clone()));
        assert_eq!(result.content.as_deref(), Some(text.as_str()), "{miss:?}");
    }
}
