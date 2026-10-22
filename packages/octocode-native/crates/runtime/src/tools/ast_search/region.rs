use crate::response::result::ToolError;
use octocode_contract_types::{AstMatchScope, AstMatchScopeMode, AstScopePoint};
use octocode_engine::{structural::StructuralDetailedMatch, text::LineIndex};

pub(super) fn select(
    scope: &AstMatchScope,
    source: &str,
    matches: &mut Vec<StructuralDetailedMatch>,
) -> Result<(), ToolError> {
    let index = LineIndex::tree_sitter(source);
    let point = |point: &AstScopePoint| {
        let invalid = || {
            ToolError::new(
                "invalidInput",
                "scope positions must be source boundaries within the file (1-based UTF-16 columns).",
            )
        };
        let row = u32::try_from(point.line.get() - 1).map_err(|_| invalid())?;
        let column = u32::try_from(point.column.get() - 1).map_err(|_| invalid())?;
        let start = index.line_start(row as usize).ok_or_else(invalid)?;
        let byte = index.position_to_byte(source, row, column) as usize;
        let prefix = source.get(start..byte).ok_or_else(invalid)?;
        let prefix = if row == 0 {
            prefix.strip_prefix('\u{feff}').unwrap_or(prefix)
        } else {
            prefix
        };
        if prefix.encode_utf16().count() != column as usize {
            return Err(invalid());
        }
        Ok((point.line.get(), point.column.get()))
    };
    let start = point(&scope.start)?;
    let end = point(&scope.end)?;
    if start >= end {
        return Err(ToolError::new(
            "invalidInput",
            "scope start must precede its end",
        ));
    }
    matches.retain(|matched| {
        let a = (
            u64::from(matched.start_line),
            u64::from(matched.start_col) + 1,
        );
        let b = (u64::from(matched.end_line), u64::from(matched.end_col) + 1);
        match scope.mode {
            AstMatchScopeMode::Contained => start <= a && b <= end,
            AstMatchScopeMode::Overlap => a < end && start < b,
        }
    });
    Ok(())
}
