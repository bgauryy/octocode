//! Parsing and range-checking the query flags.
use super::*;

/// The parsed query flags.
pub(super) struct Flags {
    pub(super) kind: Option<NodeKind>,
    pub(super) edge_kinds: Vec<EdgeKind>,
    pub(super) direction: Option<Direction>,
    pub(super) weakest: Confidence,
    pub(super) depth: Option<u32>,
}

impl Flags {
    pub(super) fn parse(options: &QueryOptions) -> Result<Self, GraphOutput> {
        let invalid = |message: String| GraphOutput::error(ErrorCode::InvalidInput, message);
        let kind = match options.kind.as_deref() {
            None => None,
            Some(value) => Some(NodeKind::parse(value).ok_or_else(|| {
                invalid(format!(
                    "unknown kind {value:?}; expected file, symbol, or package"
                ))
            })?),
        };
        let mut edge_kinds = Vec::new();
        for value in options
            .edges
            .iter()
            .flat_map(|v| v.split(','))
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            edge_kinds.push(EdgeKind::parse(value).ok_or_else(|| {
                let kinds = EdgeKind::ALL.map(EdgeKind::as_str).join(", ");
                invalid(format!("unknown edge {value:?}; expected one of: {kinds}"))
            })?);
        }
        let direction = match options.direction.as_deref() {
            None => None,
            Some("out") => Some(Direction::Out),
            Some("in") => Some(Direction::In),
            Some("both") => Some(Direction::Both),
            Some(other) => {
                return Err(invalid(format!(
                    "unknown direction {other:?}; expected out, in, or both"
                )));
            }
        };
        let weakest = match options.confidence.as_deref() {
            Some("low") => Confidence::Low,
            Some("medium") => Confidence::Medium,
            None | Some("high") => Confidence::High,
            Some(other) => {
                return Err(invalid(format!(
                    "unknown confidence {other:?}; expected high, medium, or low"
                )));
            }
        };
        let listed = matches!(options.op.as_str(), "witnesses" | "unresolved" | "symbols");
        if !options.targets.is_empty() && !listed {
            return Err(invalid(format!(
                "`{}` takes one target; several apply to witnesses, unresolved and symbols",
                options.op
            )));
        }
        if !options.reasons.is_empty() {
            if options.op != "unresolved" {
                return Err(invalid("reasons applies to unresolved".to_owned()));
            }
            let known = unresolved_reasons();
            if let Some(unknown) = options
                .reasons
                .iter()
                .find(|r| !known.contains(&r.as_str()))
            {
                return Err(invalid(format!(
                    "unknown reason {unknown:?}; expected one of: {}",
                    known.join(", ")
                )));
            }
        }
        // The contract bounds no depth: a walk ends where the graph does.
        if options.depth == Some(0) {
            return Err(invalid(
                "depth 0 is out of range; expected 1 or more".to_owned(),
            ));
        }
        // The contract bounds no limit: pages and responseChars bound a page.
        if options.limit == Some(0) {
            return Err(invalid(
                "limit 0 is out of range; expected 1 or more".to_owned(),
            ));
        }
        Ok(Self {
            kind,
            edge_kinds,
            direction,
            weakest,
            depth: options.depth,
        })
    }
}

/// The unresolved-row reason vocabulary the graphQuery contract declares.
fn unresolved_reasons() -> Vec<&'static str> {
    crate::contracts::query_schema_value(
        crate::id::ToolId::GraphQuery,
        Some("unresolved"),
        "reasons",
        "items",
    )
    .and_then(|items| items["enum"].as_array())
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .collect()
}
