//! Every private-key format through `localFetch`: whole, from inside the
//! block, across a page boundary, and by a body line's own text. The scan is
//! `Safe`, so only the block guard can have redacted them.

use super::*;

use crate::security::key_formats as formats;

/// Every page of `query` under a page `budget`, joined.
fn all_pages(mut query: LocalFetchQuery, paths: &Paths, budget: Option<usize>) -> String {
    let mut out = String::new();
    for _ in 0..200 {
        let result = execute_local_fetch(&query, paths, &Safe, &NeverCancel, budget, None);
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
        let filler = "outside\n".repeat(1_400_000);
        let path = fixture.write("large.log", format!("{filler}{}", format.text));
        let paths = Paths(fixture.0.clone());
        let lead = 1_400_000;
        let total = format.text.lines().count();
        let whole = serde_json::json!({"ranges":[{"startLine":lead + 1,"endLine":lead + total}]});
        let shown = all_pages(qj(&path, whole), &paths, None);
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
            &all_pages(qj(&path, inside), &paths, None),
            format,
            "stream inside",
        );
        let probe = serde_json::json!({"matchString":[format.secrets[1]],"contextLines":{"before":2,"after":2}});
        let result = fetch(&qj(&path, probe), &paths);
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

/// A Kubernetes Secret's values span many pages: what makes each one a secret
/// is on the first page only, or none at all, so a page's scan cannot judge it.
#[test]
fn no_page_of_a_kubernetes_secret_shows_a_value() {
    let security = crate::security::ContentSecurity::new();
    let value = |at: usize| format!("QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVow{at:06}");
    let values = (0..3_000)
        .map(|at| format!("  k{at}: {}\n", value(at)))
        .collect::<String>();
    for text in [
        format!("apiVersion: v1\nkind: Secret\nmetadata:\n  name: x\ndata:\n{values}"),
        // The order `kubectl get -o yaml` writes: the kind comes after.
        format!("apiVersion: v1\ndata:\n{values}kind: Secret\nmetadata:\n  name: x\n"),
    ] {
        let fixture = Fixture::new();
        let path = fixture.write("secret.yaml", text);
        let paths = Paths(fixture.0.clone());
        let mut query = qj(&path, serde_json::json!({}));
        let mut pages = 0;
        loop {
            let page =
                execute_local_fetch(&query, &paths, &security, &NeverCancel, Some(2_000), None);
            let content = page.content.as_deref().unwrap_or_default();
            assert!(!content.contains("QUJDREVGR0hJ"), "{content}");
            pages += 1;
            match page.next.and_then(|next| next.continue_window) {
                Some(next) => query = next.query,
                None => break,
            }
        }
        assert!(pages >= 1);
        let range = serde_json::json!({"ranges":[{"startLine":2_500,"endLine":2_501}]});
        let page = execute_local_fetch(
            &qj(&path, range),
            &paths,
            &security,
            &NeverCancel,
            None,
            None,
        );
        assert!(
            !page
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("QUJDREVGR0hJ")
        );
    }
}

/// Past the whole-file ceiling no scan sees the kind and a far value together:
/// the window hides what a Secret's value looks like, and nothing else.
#[test]
fn a_streamed_yaml_window_shows_no_value_a_secret_could_hold() {
    let security = crate::security::ContentSecurity::new();
    let value = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";
    let filler = format!("# {}\n", "p".repeat(70)).repeat(150_000);
    let fixture = Fixture::new();
    let text = format!("{filler}kind: Secret\ndata:\n  tls.crt: {value}\n  name: short\n");
    let path = fixture.write("large.yaml", text);
    let paths = Paths(fixture.0.clone());
    let at = 150_000 + 3;
    for query in [
        serde_json::json!({"ranges":[{"startLine":at,"endLine":at + 1}]}),
        serde_json::json!({"matchString":[&value[..16]],"contextLines":{"before":1,"after":1}}),
    ] {
        let page = execute_local_fetch(
            &qj(&path, query.clone()),
            &paths,
            &security,
            &NeverCancel,
            None,
            None,
        );
        let content = page.content.as_deref().unwrap_or_default();
        assert!(!content.contains(&value[..12]), "{query}: {content}");
    }
    let range = serde_json::json!({"ranges":[{"startLine":at,"endLine":at + 1}]});
    let page = execute_local_fetch(
        &qj(&path, range),
        &paths,
        &security,
        &NeverCancel,
        None,
        None,
    );
    assert!(
        page.content
            .as_deref()
            .unwrap_or_default()
            .contains("name: short")
    );
}
