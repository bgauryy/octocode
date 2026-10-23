//! Pages cut to fit the row's share of a small response window.
use super::*;

/// The unit a query pages in: `unit`, else bytes for a whole-file read and
/// lines otherwise.
fn paged_unit(query: &LocalFetchQuery) -> WindowUnit {
    query.unit.unwrap_or(if query.full_content == Some(true) {
        WindowUnit::Bytes
    } else {
        WindowUnit::Lines
    })
}

/// A file of short lines read under a window too small for the whole file
/// is cut to whole-line pages: each page states the unit its
/// `continueWindow` pages in, no page calls the file one long line, and the
/// walk reaches every byte exactly once.
#[test]
fn a_page_cut_to_a_small_window_states_the_unit_it_pages_in() {
    let math = "export const add = (a: number, b: number) => a + b;\nexport const sub = (a: number, b: number) => a - b;\n";
    let lines = (1..=41)
        .map(|n| format!("export const value{n} = {n};\n"))
        .collect::<String>();
    for (source, full_content) in [math, &lines]
        .into_iter()
        .flat_map(|source| [(source, None), (source, Some(true))])
    {
        for window in (40..=600).step_by(8) {
            let facts = SourceFacts {
                window: Some(window),
                ..SourceFacts::default()
            };
            let mut query = LocalFetchQuery {
                path: "/fixture/math.ts".parse().expect("path"),
                full_content,
                ..test_local_fetch_query()
            };
            let mut walked = String::new();
            for _ in 0..=source.len() {
                let page = process_fetched_content(
                    &query,
                    source.as_bytes(),
                    Path::new("/fixture/math.ts"),
                    &facts,
                    &security(),
                    &NeverCancel,
                );
                assert_eq!(page.status, ReadStatus::Success, "{window}: {page:?}");
                assert!(
                    !page
                        .warnings
                        .iter()
                        .any(|w| w.starts_with("One line fills")),
                    "{window} {full_content:?}: {:?}",
                    page.warnings
                );
                walked.push_str(page.content.as_deref().unwrap_or_default());
                let pagination = page.pagination.expect("pagination");
                let Some(next) = page.next.and_then(|next| next.continue_window) else {
                    break;
                };
                assert_eq!(
                    pagination.unit,
                    paged_unit(&next.query),
                    "{window} {full_content:?}: {pagination:?} pages on with {:?}",
                    next.query
                );
                query = next.query;
            }
            assert_eq!(walked, source, "{window} {full_content:?}");
        }
    }
}
