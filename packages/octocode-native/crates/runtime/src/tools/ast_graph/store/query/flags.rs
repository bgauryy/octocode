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
                    "unknown --kind {value:?}; expected file, symbol, or package"
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
                invalid(format!(
                    "unknown --edge {value:?}; expected one of: {kinds}"
                ))
            })?);
        }
        let direction = match options.direction.as_deref() {
            None => None,
            Some("out") => Some(Direction::Out),
            Some("in") => Some(Direction::In),
            Some("both") => Some(Direction::Both),
            Some(other) => {
                return Err(invalid(format!(
                    "unknown --direction {other:?}; expected out, in, or both"
                )));
            }
        };
        let weakest = match options.confidence.as_deref() {
            Some("low") => Confidence::Low,
            Some("medium") => Confidence::Medium,
            None | Some("high") => Confidence::High,
            Some(other) => {
                return Err(invalid(format!(
                    "unknown --confidence {other:?}; expected high, medium, or low"
                )));
            }
        };
        if let Some(depth) = options
            .depth
            .filter(|depth| !(1..=MAX_DEPTH).contains(depth))
        {
            return Err(invalid(format!(
                "--depth {depth} is out of range; expected 1-{MAX_DEPTH}"
            )));
        }
        if let Some(limit) = options
            .limit
            .filter(|limit| !(1..=MAX_LIMIT).contains(limit))
        {
            return Err(invalid(format!(
                "--limit {limit} is out of range; expected 1-{MAX_LIMIT}"
            )));
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
