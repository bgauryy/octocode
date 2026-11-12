//! Explain response-budget cuts without implying that source evidence was lost.
use crate::tools::source_view::{local_fetch_query, types::Pagination};
use octocode_contract_types::{LocalFetchQuery, ReadProgressUnit};

/// Why a line page ended short of the caller's `limit` with more to read:
/// the response budget sized it (`responseChars`), nothing was lost, and
/// `next.nextPage` reads on. `None` when no `limit` was asked, or the
/// page reached it.
pub(super) fn budget_cut_hint(
    read: &LocalFetchQuery,
    pagination: &Pagination,
    out_of_range: bool,
) -> Option<String> {
    let limit = local_fetch_query::line_limit(read);
    (pagination.unit == ReadProgressUnit::Lines
        && pagination.has_more
        && !out_of_range
        && limit != usize::MAX
        && pagination.length < limit)
        .then(|| {
            format!(
                "This page holds {}, short of limit {limit}: the response budget sized it, no line is lost; next reads on, and a larger responseChars lengthens the page.",
                octocode_engine::text::counted(pagination.length, "line", "lines")
            )
        })
}

#[cfg(test)]
mod budget_cut_hint_tests {
    use super::*;

    /// One line is "1 line", and any other count its plural.
    #[test]
    fn the_hint_counts_a_short_page_in_the_right_number() {
        let read: LocalFetchQuery =
            serde_json::from_value(serde_json::json!({"path":"a.rs","limit":5}))
                .expect("a read with a limit");
        let hint = |length| {
            budget_cut_hint(
                &read,
                &Pagination {
                    unit: ReadProgressUnit::Lines,
                    offset: 0,
                    length,
                    window: length,
                    total_lines: 9,
                    total_bytes: 90,
                    has_more: true,
                    next_offset: Some(length),
                    resume_line: None,
                    placed: None,
                },
                false,
            )
            .expect("a short page")
        };
        assert!(
            hint(1).starts_with("This page holds 1 line, "),
            "{}",
            hint(1)
        );
        assert!(
            hint(2).starts_with("This page holds 2 lines, "),
            "{}",
            hint(2)
        );
    }
}
