//! The SCIP precise tier at a call site: the declaration an index names for
//! the call's cited name token.
use super::*;
use crate::ast_graph::types::Call;

impl Linker<'_> {
    /// The declaration the SCIP index says `call` calls, for a Rust call the
    /// facts record: the reference occurrence at its cited name must name
    /// exactly one mapped declaration that can be called. `None` leaves the
    /// syntax link as it is.
    pub(super) fn scip_target(
        &self,
        unit: &FileLinks,
        call: &Call,
        callable: &dyn Fn(u32) -> bool,
        method: &dyn Fn(u32) -> bool,
    ) -> Option<u32> {
        let index = self.scip?;
        if unit.language != "rust" || !matches!(call.kind.as_str(), "" | "calls" | "call") {
            return None;
        }
        let span = call.source_range.filter(|span| span.unit.0 == 0)?;
        let file = *self.scope.file_paths.get(unit.file_id as usize)?;
        let name = cited_name(&call.callee)?;
        let line0 = call.line.checked_sub(1)?;
        let dst = index.call_target(file, line0, (span.start_byte, span.end_byte), name)?;
        (callable(dst) || method(dst)).then_some(dst)
    }
}

/// The name a call cites first, as the extractor cites it: a callee's own
/// last segment, or for a chain row (`read(..).map_err`) its head call.
fn cited_name(callee: &str) -> Option<&str> {
    let head = callee.split_once('(').map_or(callee, |(head, _)| head);
    head.rsplit(['.', ':', '>'])
        .find(|segment| !segment.is_empty())
}
