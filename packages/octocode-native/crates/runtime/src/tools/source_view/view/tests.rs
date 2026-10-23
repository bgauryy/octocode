//! Unit tests of the view pipeline: paging, line windows and selection binding.

use super::*;
use crate::execution::cancel::NeverCancel;
use crate::security::ContentSecurity;
use octocode_contract_types::ErrorCode;
use std::path::Path;

fn security() -> ContentSecurity {
    ContentSecurity::new()
}
fn fetch(source: &str, q: &LocalFetchQuery) -> LocalFetchResult {
    process_fetched_content(
        q,
        source.as_bytes(),
        Path::new("/fixture/app.ts"),
        &SourceFacts::default(),
        &security(),
        &NeverCancel,
    )
}
fn lines_query(offset: usize, chunk: usize) -> LocalFetchQuery {
    LocalFetchQuery {
        path: "/fixture/app.ts".parse().expect("path"),
        unit: Some(WindowUnit::Lines),
        offset: Some(wire_count(offset)),
        length: wire_positive(chunk),
        ..test_local_fetch_query()
    }
}
fn filler(n: usize) -> String {
    (1..=n)
        .map(|i| format!("const filler_{i} = {i};\n"))
        .collect()
}

#[test]
fn fetch_partition_binding_rejects_same_count_legacy_replay() {
    use crate::execution::SelectionLimits;
    use crate::tools::selection::{Candidate, Decision, JudgeCall, Runtime, SelectionJudge};
    use octocode_contract_types::SelectionUnit;
    use serde_json::json;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};

    struct FirstCandidate(Arc<AtomicUsize>);
    impl SelectionJudge for FirstCandidate {
        fn identity(&self) -> serde_json::Value {
            json!({"test":"fetch-partition"})
        }
        fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(call
                .admitted
                .iter()
                .map(|&ordinal| {
                    (
                        ordinal,
                        if ordinal == 0 {
                            Decision::Selected
                        } else {
                            Decision::Excluded
                        },
                    )
                })
                .collect())
        }
    }

    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = Runtime {
        limits: SelectionLimits {
            max_candidates: 64,
            max_chars: 20_000,
            timeout: Duration::from_secs(5),
        },
        available: true,
        judge: Arc::new(FirstCandidate(calls.clone())),
        handle: executor.handle().clone(),
        context: crate::execution::ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: Instant::now() + Duration::from_secs(5),
            walk_threads: None,
            response_window: None,
            github_credential: None,
        },
    };
    let first = format!("{}\n", "a".repeat(999));
    let second = format!("{}\n", "b".repeat(999));
    let third = format!("{}\n", "c".repeat(2_047));
    let source = (1..=20)
        .map(|line| match line {
            1 => first.as_str(),
            10 => second.as_str(),
            20 => third.as_str(),
            _ => "gap\n",
        })
        .collect::<String>();
    let query: LocalFetchQuery = serde_json::from_value(json!({
        "path":"/fixture/source.txt", "ranges":["1-1","10-10","20-20"], "clasifyContext":{"classifiers":["Find the first evidence"]}
    }))
    .expect("query");
    let path = Path::new("/fixture/source.txt");
    let sha256 = octocode_engine::digest::sha256(source.as_bytes());
    let input = SelectionInput {
        q: &query,
        source_path: path,
        provider_path: "/fixture/source.txt",
        sha256: &sha256,
        runtime: Some(&runtime),
        query_json: std::cell::OnceCell::new(),
    };
    let extracted = extract(&query, &source).unwrap_or_else(|_| panic!("source extraction"));
    let (current, _) = crate::tools::source_view::selection::chunks(
        &extracted.text,
        extracted.source_lines.as_deref(),
        Some(&source),
        path,
        input.provider_path,
        &security(),
        Some(&runtime),
        None,
        None,
    );
    let legacy = [
        Candidate::new(Some(
            json!({"content":format!("{first}{}{second}", crate::tools::source_view::extraction::omission_marker(2, 9))}),
        )),
        Candidate::new(Some(
            json!({"content":crate::tools::source_view::extraction::omission_marker(11, 19)}),
        )),
        Candidate::new(Some(json!({"content":third}))),
    ];
    assert_eq!(current.len(), legacy.len());
    assert_eq!(current[0].state.as_ref().unwrap()["content"], first);
    assert!(
        legacy[0].state.as_ref().unwrap()["content"]
            .as_str()
            .unwrap()
            .contains(&second)
    );
    let query_json = serde_json::to_value(&query).expect("query JSON");
    let select = |source: &str, prior: Option<&str>, candidates: &[Candidate]| {
        crate::tools::selection::select(
            Some(&runtime),
            &query_json,
            source,
            prior,
            candidates,
            None,
            SelectionUnit::Chunks,
        )
    };
    let old = select(&format!("{}:{sha256}", path.display()), None, &legacy)
        .unwrap_or_else(|_| panic!("legacy plan"));
    let replay = (0..current.len())
        .map(|_| Candidate::new(None))
        .collect::<Vec<_>>();
    assert!(matches!(
        select(&input.source_key(), Some(&old.token), &replay),
        Err(crate::tools::selection::Failure::Stale)
    ));
    let fresh =
        select(&input.source_key(), None, &current).unwrap_or_else(|_| panic!("fresh plan"));
    let resumed = select(&input.source_key(), Some(&fresh.token), &replay)
        .unwrap_or_else(|_| panic!("fresh replay"));
    assert_eq!(resumed.selected, [0]);
    assert_eq!(resumed.token, fresh.token);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn selected_reads_fill_the_configured_window_and_keep_explicit_lengths() {
    let source = filler(1_200);
    let mut query = LocalFetchQuery {
        path: "/fixture/app.ts".parse().expect("path"),
        ranges: vec!["1-1200".parse().expect("range")],
        ..test_local_fetch_query()
    };
    let read = |query: &LocalFetchQuery| {
        process_fetched_content(
            query,
            source.as_bytes(),
            Path::new("/fixture/app.ts"),
            &SourceFacts {
                window: Some(50_000),
                ..SourceFacts::default()
            },
            &security(),
            &NeverCancel,
        )
    };
    let complete = read(&query);
    assert_eq!(complete.status, ReadStatus::Success);
    assert!(
        complete
            .content
            .as_deref()
            .expect("content")
            .contains("filler_1200")
    );
    assert!(!complete.pagination.as_ref().expect("pagination").has_more);

    query.unit = Some(WindowUnit::Lines);
    query.length = wire_positive(17);
    let explicit = read(&query);
    assert_eq!(explicit.pagination.as_ref().expect("pagination").length, 17);
    assert!(explicit.pagination.as_ref().expect("pagination").has_more);
}

/// A read with no anchor whose view would not fit one page returns a
/// short head instead of the page: the read is not targeted yet. Paging
/// on, whole-file reads and anchored reads are unchanged; a file that
/// fits one page is returned whole, with no note.
#[test]
fn an_unanchored_read_over_one_page_returns_a_head() {
    // 1,000 short lines: far below any line-count ceiling, over 16 KiB.
    let source = filler(1_000);
    assert!(source.len() > 16 * 1024);
    let plain = LocalFetchQuery {
        path: "/fixture/app.ts".parse().expect("path"),
        ..test_local_fetch_query()
    };
    let head = fetch(&source, &plain);
    assert_eq!(head.returned_lines, Some(HEAD_LINES), "{head:?}");
    // The head's facts are data (totalLines, continueWindow), not a note.
    assert!(head.warnings.is_empty(), "{:?}", head.warnings);
    assert_eq!(head.total_lines, Some(1_000), "{head:?}");
    let next = head
        .next
        .and_then(|next| next.continue_window)
        .expect("continueWindow");
    assert_eq!(local_fetch_query::offset(&next.query), Some(HEAD_LINES));
    assert_eq!(local_fetch_query::window_length(&next.query), None);
    let page = fetch(&source, &next.query);
    assert!(page.returned_lines.expect("lines") > HEAD_LINES, "{page:?}");
    // Whole-file, anchored and symbol reads are unchanged.
    let whole = fetch(
        &source,
        &LocalFetchQuery {
            full_content: Some(true),
            ..plain.clone()
        },
    );
    assert!(whole.returned_lines.expect("lines") > HEAD_LINES);
    let ranged = fetch(
        &source,
        &LocalFetchQuery {
            ranges: vec![format!("{}-{}", 10, 200).parse().expect("range")],
            ..plain.clone()
        },
    );
    assert_eq!(ranged.returned_lines, Some(191));
    // A standard view is guarded the same way (ghGetFileContent's default).
    let standard = fetch(
        &source,
        &LocalFetchQuery {
            minify: Some(MinifyMode::Standard),
            ..plain.clone()
        },
    );
    assert_eq!(standard.returned_lines, Some(HEAD_LINES), "{standard:?}");
    // A file that fits one page is read whole, with no note.
    let small = fetch(&filler(500), &plain);
    assert_eq!(small.returned_lines, Some(500), "{small:?}");
    assert!(small.warnings.is_empty(), "{:?}", small.warnings);
    assert!(small.next.is_none(), "{small:?}");
}

mod fit;
mod pages;
