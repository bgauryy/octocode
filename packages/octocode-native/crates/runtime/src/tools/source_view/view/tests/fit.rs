//! Pages cut to fit the row's share of a small response window.
use super::*;
use octocode_contract_types::LocalFetchQuery;

/// A file of short lines read under a window too small for the whole file
/// is cut to whole-line pages: each page states the unit its
/// `nextPage` pages in, no page calls the file one long line, and the
/// walk reaches every byte exactly once.
#[test]
fn a_page_cut_to_a_small_window_states_the_unit_it_pages_in() {
    let math = "export const add = (a: number, b: number) => a + b;\nexport const sub = (a: number, b: number) => a - b;\n";
    let lines = (1..=41)
        .map(|n| format!("export const value{n} = {n};\n"))
        .collect::<String>();
    for source in [math, &lines] {
        for window in (40..=600).step_by(8) {
            let facts = SourceFacts {
                window: Some(window),
                ..SourceFacts::default()
            };
            let mut query: LocalFetchQuery =
                serde_json::from_value(serde_json::json!({"path":"/fixture/math.ts"}))
                    .expect("canonical read");
            let mut walked = String::new();
            for _ in 0..=source.len() {
                let page = process_fetched_content(
                    &query,
                    source.as_bytes(),
                    Path::new("/fixture/math.ts"),
                    &facts,
                    &ContentSecurity::new(),
                    &NeverCancel,
                );
                assert_eq!(page.status, ReadStatus::Success, "{window}: {page:?}");
                assert!(
                    !page
                        .warnings
                        .iter()
                        .any(|w| w.starts_with("One line fills")),
                    "{window}: {:?}",
                    page.warnings
                );
                walked.push_str(page.content.as_deref().unwrap_or_default());
                let pagination = page.pagination.expect("pagination");
                let Some(next) = page.next.and_then(|next| next.continue_window) else {
                    break;
                };
                assert_eq!(
                    pagination.unit,
                    local_fetch_query::unit(&next.query),
                    "{window}: {pagination:?} pages on with {:?}",
                    next.query
                );
                query = next.query;
            }
            assert_eq!(walked, source, "{window}");
        }
    }
}
