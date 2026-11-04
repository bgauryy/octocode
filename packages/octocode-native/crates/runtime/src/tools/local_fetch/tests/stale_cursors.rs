//! A cursor names the source version it read. Once the file changed, every
//! kind of later page answers `staleSnapshot`, whatever the new bytes would
//! select: no hit past the page is not an empty page of the old read.
use super::*;

const LINE: usize = 80;
const HIT_EVERY: usize = 40;

/// Numbered lines of [`LINE`] bytes, every [`HIT_EVERY`]th holding a hit, about
/// `bytes` long.
fn source(bytes: usize) -> String {
    (0..bytes / LINE)
        .map(|index| {
            let word = if index % HIT_EVERY == 0 {
                "needle"
            } else {
                "other "
            };
            format!("{index:072} {word}\n")
        })
        .collect()
}

/// `source` with only its first hit left, the same size: no hit past a page.
fn first_hit_only(source: &str) -> String {
    let after = source.find("needle").expect("a hit") + "needle".len();
    format!(
        "{}{}",
        &source[..after],
        source[after..].replace("needle", "noodle")
    )
}

/// `source` with a NUL past the sniffed head, the same size.
fn nul_past_the_head(source: &str) -> String {
    let at = source.len() - 2;
    format!("{}\0{}", &source[..at], &source[at + 1..])
}

/// The offered call that reads on with the same kind of read.
fn later_page(first: &LocalFetchResult, matches: bool) -> LocalFetchQuery {
    let next = first.next.as_ref().expect("a later page");
    [&next.continue_window, &next.read_other_ranges]
        .into_iter()
        .flatten()
        .map(|call| call.query.clone())
        .find(|query| !matches || !local_fetch_query::match_strings(query).is_empty())
        .expect("a later page of this read")
}

/// What each kind of later page answers once the file changed, one line per
/// case that was not `staleSnapshot`.
fn answers_after_changes(
    bytes: usize,
    settled: &[bool],
    changes: &[(&str, String)],
) -> Vec<String> {
    let fixture = Fixture::new();
    let original = source(bytes);
    let last = original.lines().count();
    let paths = Paths(fixture.0.clone());
    let reads = [
        ("match", serde_json::json!({"matchString":["needle"]})),
        (
            "ranges",
            serde_json::json!({"ranges":[{"startLine":1,"endLine":last}]}),
        ),
        ("lines", serde_json::json!({})),
    ];
    let mut wrong = Vec::new();
    for &settle in settled {
        for (read, fields) in &reads {
            for (change, changed) in changes {
                let file = fixture.write("read.log", &original);
                if settle {
                    octocode_engine::stamp::settle_written_so_far();
                }
                let page = |query: &LocalFetchQuery| {
                    execute_local_fetch(query, &paths, &Safe, &NeverCancel, Some(2_000), None)
                };
                let first = page(&qj(&file, fields.clone()));
                assert_eq!(first.error_code, None, "{read}: {:?}", first.error);
                let next = later_page(&first, *read == "match");
                fs::write(&file, changed).expect("change");
                let answer = page(&next);
                if answer.error_code != Some(ErrorCode::StaleSnapshot) {
                    wrong.push(format!(
                        "{read}, {change}, settled {settle}: {:?} {:?}",
                        answer.status, answer.error_code
                    ));
                }
            }
        }
    }
    wrong
}

#[test]
fn every_later_page_of_a_changed_file_is_stale_in_memory() {
    let original = source(200_000);
    let wrong = answers_after_changes(
        200_000,
        &[false],
        &[
            ("no hit past the page", first_hit_only(&original)),
            ("shrunk", "nothing to find\n".to_owned()),
        ],
    );
    assert_eq!(wrong, Vec::<String>::new());
}

#[test]
fn every_later_page_of_a_changed_file_is_stale_when_streamed() {
    let bytes = super::super::acquire::MAX_SOURCE_BYTES as usize + 1024 * 1024;
    let original = source(bytes);
    let wrong = answers_after_changes(
        bytes,
        &[false, true],
        &[
            ("no hit past the page", first_hit_only(&original)),
            ("shrunk", "nothing to find\n".to_owned()),
            ("a NUL past the head", nul_past_the_head(&original)),
        ],
    );
    assert_eq!(wrong, Vec::<String>::new());
}
