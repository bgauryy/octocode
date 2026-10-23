//! The source lines behind a transformed view, so its pages count raw lines.
use crate::tools::source_view::extraction::OMISSION_LINE;

/// The lines behind a transformed view (the standard view drops blank and
/// comment lines) in the unit a raw read of the same selection pages in:
/// each view line's 1-based line of the selected text, ascending, and that
/// text's line count. A whole-file view's selected text is the source.
pub(in crate::tools::source_view) struct SourceLines {
    pub(in crate::tools::source_view) lines: Vec<usize>,
    pub(in crate::tools::source_view) total: usize,
}

impl SourceLines {
    /// The source line of every line of a gutter-numbered view (`N\t…`);
    /// `None` when a line carries no number or the numbers do not ascend.
    pub(in crate::tools::source_view) fn of_gutter(view: &str, total: usize) -> Option<Self> {
        let mut lines = Vec::new();
        for record in view.split_inclusive('\n') {
            let (number, _) = record.split_once(crate::response::numbered::SEPARATOR)?;
            let line = number.parse::<usize>().ok()?;
            if lines.last().is_some_and(|last| *last >= line) || line > total {
                return None;
            }
            lines.push(line);
        }
        Some(Self { lines, total })
    }

    /// [`Self::of_gutter`] for a view of an extraction (ranges): `selected`
    /// maps each line of the extracted text to its source line
    /// ([`OMISSION_LINE`] for an omission marker), so a gutter line counts
    /// as its line of the extracted text and a gap line
    /// ([`crate::response::numbered::GAP`]) as its marker. `None` when a view
    /// line maps onto no extracted line.
    pub(in crate::tools::source_view) fn of_selected_gutter(
        view: &str,
        selected: &[usize],
    ) -> Option<Self> {
        let mut lines = Vec::new();
        let mut next = 0;
        for record in view.split_inclusive('\n') {
            let wanted = if record == crate::response::numbered::GAP {
                OMISSION_LINE
            } else {
                let (number, _) = record.split_once(crate::response::numbered::SEPARATOR)?;
                number.parse::<usize>().ok()?
            };
            let at = next
                + selected
                    .get(next..)?
                    .iter()
                    .position(|line| *line == wanted)?;
            lines.push(at + 1);
            next = at + 1;
        }
        Some(Self {
            lines,
            total: selected.len(),
        })
    }
}
