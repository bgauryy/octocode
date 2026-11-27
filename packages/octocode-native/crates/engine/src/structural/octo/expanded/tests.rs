use super::*;

/// The extent audit every engine test runs: a match from a body lies inside
/// the body's text and agrees with every flat bracket group of its owner (it
/// spans a group, or lies inside it without touching its delimiters, or is
/// apart from it).
/// A violation aborts the test binary (a panic could be caught as a failed
/// search), naming the node.
pub(super) fn audit_extent(body: &Body, node: Node<'_>) {
    let span = node.byte_range();
    let inside_text = body.range.start_byte <= span.start && span.end <= body.range.end_byte;
    let agrees = |group: &Span| {
        let apart = span.end <= group.start || group.end <= span.start;
        let spans = span.start <= group.start && group.end <= span.end;
        let within = group.start < span.start && span.end < group.end;
        apart || spans || within
    };
    if !(inside_text && body.audit_groups.iter().all(agrees)) {
        use std::io::Write as _;
        let _ = writeln!(
            std::io::stderr(),
            "extent audit: a {} match at {span:?} disagrees with its body {:?} ({} line {}) or a bracket group of it",
            node.kind(),
            body.range.start_byte..body.range.end_byte,
            body.disclosed.name,
            body.disclosed.line,
        );
        std::process::abort();
    }
}
