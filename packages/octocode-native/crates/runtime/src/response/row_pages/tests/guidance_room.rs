//! A split row's continuations ride its last part only, so the parts before
//! it are packed without reserving room for them.
use super::*;
use tokio_util::sync::CancellationToken;

/// A code-search row as `ghSearchCode` lists it: `files` hit rows and the
/// row's `next` reads (the next provider page and each file's unlisted hits).
fn code_search_row(files: usize, reads: usize) -> Value {
    let cursor = |n: usize| json!({"kind":"continue","tool":"ghGetFileContent","input":{"cursor":format!("cursor1.{n:0>64}")}});
    let next = (0..reads)
        .map(|n| (format!("readHits{n}"), cursor(n)))
        .collect::<Map<_, _>>();
    let files = (0..files)
        .map(|n| {
            json!({"owner":"o","repo":"r","path":format!("src/module_{n:03}.js"),
                "matches":[{"value":format!("  useEffect(() => {{ run_{n}(); }}, [dep_{n}]);\n").repeat(4),
                    "matchIndices":[{"start":2,"end":11,"lineOffset":0}]}],
                "lines":[format!("{n}\t  useEffect(() => {{ run_{n}(); }}, [dep_{n}]);")]})
        })
        .collect::<Vec<_>>();
    json!({"index":0,"data":{"files":files,"pagination":{"page":1,"totalPages":6,"pageSize":50,"totalItems":259,"hasMore":true},"next":next}})
}

#[test]
fn parts_before_the_last_fill_the_room_the_continuations_leave_them() {
    let row = code_search_row(40, 8);
    let files_chars: usize = row["data"]["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| octocode_engine::json_size::utf16_units(file).unwrap_or(0) + 1)
        .sum();
    let largest_file = row["data"]["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| octocode_engine::json_size::utf16_units(file).unwrap_or(0) + 1)
        .max()
        .unwrap_or(0);
    let next_chars = octocode_engine::json_size::utf16_units(&row["data"]["next"]).unwrap_or(0);
    let budget = 3_000;
    let plan = PreparedRows::new(vec![row], &[None], Some(crate::id::ToolId::GhSearchCode))
        .plan(budget, &CancellationToken::new())
        .expect("valid plan");
    // Every part repeats a small skeleton; only the last carries `next`.
    let ceiling = (files_chars + next_chars).div_ceil(budget - 2 * largest_file) + 1;
    assert!(
        plan.total_pages <= ceiling,
        "{} pages for {files_chars} chars of files and {next_chars} of next at {budget}; at most {ceiling}",
        plan.total_pages
    );
    let mut delivered = Vec::new();
    for page in 0..plan.total_pages {
        let rows = plan
            .build_page(page, &[None], &[], &CancellationToken::new())
            .expect("page");
        for row in &rows {
            let size = octocode_engine::json_size::utf16_units(row).unwrap_or(0);
            assert!(size <= budget, "page {page} holds {size} chars: {row}");
            let last = page + 1 == plan.total_pages;
            // A part before the last had no room for one more file; the one
            // just before it, none for a file beside the last part's `next`.
            let tail = if page + 2 == plan.total_pages {
                next_chars
            } else {
                0
            };
            assert!(
                last || size + largest_file + tail > budget,
                "page {page} stopped at {size} of {budget} chars"
            );
            assert_eq!(row["data"].get("next").is_some(), last, "{row}");
            delivered.extend(
                row["data"]["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|file| file["path"].clone()),
            );
        }
    }
    assert_eq!(delivered.len(), 40, "every file exactly once");
}
