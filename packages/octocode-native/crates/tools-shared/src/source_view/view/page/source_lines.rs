//! Project a returned page onto its selected source lines.
use crate::source_view::pagination::Page;
use octocode_contract_types::ReadView;

/// The source lines a page touches: a raw page's through the view-to-source
/// line map (or its own lines), a transformed view's from its gutter.
pub(super) fn page_lines(
    pg: &Page,
    source_lines: Option<&[usize]>,
    mode: Option<ReadView>,
) -> Vec<usize> {
    if mode.is_some() {
        return pg
            .text
            .lines()
            .filter_map(|line| {
                let (number, _) =
                    line.split_once(octocode_runtime_base::response::numbered::SEPARATOR)?;
                number.split('-').next()?.parse().ok()
            })
            .collect();
    }
    let (first, last) = pg.view_lines;
    match source_lines {
        Some(lines) => lines
            .get(first.saturating_sub(1)..last.min(lines.len()))
            .unwrap_or_default()
            .to_vec(),
        None => (first..=last).collect(),
    }
}
