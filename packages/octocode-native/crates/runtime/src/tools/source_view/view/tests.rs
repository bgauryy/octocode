use super::*;
use crate::execution::cancel::NeverCancel;
use crate::security::ContentSecurity;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use serde_json::json;
use std::path::Path;

fn query(fields: serde_json::Value) -> LocalFetchQuery {
    let mut row = json!({"path":"/fixture/app.ts"});
    row.as_object_mut()
        .expect("query")
        .extend(fields.as_object().expect("fields").clone());
    serde_json::from_value(row).expect("canonical reader query")
}

fn fetch_window(source: &str, q: &LocalFetchQuery, window: usize) -> LocalFetchResult {
    process_fetched_content(
        q,
        source.as_bytes(),
        Path::new("/fixture/app.ts"),
        &SourceFacts {
            window: Some(window),
            ..SourceFacts::default()
        },
        &ContentSecurity::new(),
        &NeverCancel,
    )
}

fn filler(n: usize) -> String {
    (1..=n)
        .map(|i| format!("const filler_{i} = {i};\n"))
        .collect()
}

fn walk(source: &str, mut q: LocalFetchQuery, window: usize) -> (String, usize) {
    let mut joined = String::new();
    let mut pages = 0;
    loop {
        let result = fetch_window(source, &q, window);
        assert!(result.error.is_none(), "{result:?}");
        joined.push_str(result.content.as_deref().expect("content"));
        pages += 1;
        assert!(pages < source.len() + 2, "progress must terminate");
        let Some(next) = result.next.and_then(|next| next.continue_window) else {
            break;
        };
        assert_ne!(next.query, q, "continuation must advance");
        if local_fetch_query::unit(&next.query) == local_fetch_query::unit(&q) {
            assert!(
                local_fetch_query::offset(&next.query).unwrap_or(0)
                    > local_fetch_query::offset(&q).unwrap_or(0)
            );
        }
        q = next.query;
    }
    (joined, pages)
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
        "path":"/fixture/source.txt", "ranges":[{"startLine":1,"endLine":1},{"startLine":10,"endLine":10},{"startLine":20,"endLine":20}], "classification":{"predicates":["Find the first evidence"]}
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
        &ContentSecurity::new(),
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
fn default_read_and_explicit_ranges_reconstruct_complete_source() {
    let source = filler(1_200);
    for fields in [
        json!({}),
        json!({"ranges":[{"startLine":1,"endLine":1200}]}),
    ] {
        let (actual, pages) = walk(&source, query(fields), 1200);
        assert_eq!(actual, source);
        assert!(pages > 1);
    }
}

#[test]
fn an_unanchored_read_uses_the_response_budget_and_small_files_finish() {
    let source = filler(1_200);
    let first = fetch_window(&source, &query(json!({})), 50_000);
    assert_eq!(first.content.as_deref(), Some(source.as_str()));
    assert!(first.next.is_none());
    assert_eq!(first.returned_lines, Some(1200));
    let small = filler(10);
    let out = fetch_window(&small, &query(json!({})), 1200);
    assert_eq!(out.content.as_deref(), Some(small.as_str()));
    assert!(out.next.is_none());
}

mod fit;
mod pages;
