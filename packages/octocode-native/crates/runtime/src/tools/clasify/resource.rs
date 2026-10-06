//! A clasify resource's delegated read, identified and parsed once.
//!
//! Clasify branches on what a read asks for (its tool, an operation, list
//! discovery versus exact lookup, the scope a row falls back to). Those
//! decisions read the generated `<Tool>Query` types after the same contract
//! preparation the read itself runs. The read and every echoed continuation
//! keep the query as sent: the generated types serialize their defaults, so
//! a round trip would change what the caller copies.
use crate::contracts::tool_types::{
    ArtifactSearchQuery, AstSearchQuery, GhSearchCodeQuery, GhSearchHistoryQuery, LspSearchQuery,
};
use crate::tools::id::ToolId;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// The read tool a `{tool, query}` source names.
pub(crate) fn tool_of(source: &Value) -> Option<ToolId> {
    source
        .get("tool")
        .and_then(Value::as_str)
        .and_then(ToolId::from_name)
}

/// A delegated read's query in its tool's generated type, for the tools
/// whose query clasify branches on. Other reads are known by tool alone.
pub enum ResourceSource {
    AstSearch(Box<AstSearchQuery>),
    LspSearch(Box<LspSearchQuery>),
    GhSearchCode(Box<GhSearchCodeQuery>),
    GhSearchHistory(Box<GhSearchHistoryQuery>),
    ArtifactSearch(Box<ArtifactSearchQuery>),
    Other(ToolId),
}

impl ResourceSource {
    /// Parse a `{tool, query}` source. `None` for a supplied value, an
    /// unknown tool, or a query its tool's contract rejects: that read fails
    /// with the contract's own error when it runs.
    pub(crate) fn of(source: &Value) -> Option<Self> {
        let tool = tool_of(source)?;
        let typed =
            |query: &Value| -> Option<Value> { super::context::prepare(tool.as_str(), query).ok() };
        let query = source.get("query").unwrap_or(&Value::Null);
        Some(match tool {
            ToolId::AstSearch => Self::AstSearch(parse(typed(query)?)?),
            ToolId::LspSearch => Self::LspSearch(parse(typed(query)?)?),
            ToolId::GhSearchCode => Self::GhSearchCode(parse(typed(query)?)?),
            ToolId::GhSearchHistory => Self::GhSearchHistory(parse(typed(query)?)?),
            ToolId::ArtifactSearch => Self::ArtifactSearch(parse(typed(query)?)?),
            other => Self::Other(other),
        })
    }

    pub(crate) fn tool(&self) -> ToolId {
        match self {
            Self::AstSearch(_) => ToolId::AstSearch,
            Self::LspSearch(_) => ToolId::LspSearch,
            Self::GhSearchCode(_) => ToolId::GhSearchCode,
            Self::GhSearchHistory(_) => ToolId::GhSearchHistory,
            Self::ArtifactSearch(_) => ToolId::ArtifactSearch,
            Self::Other(tool) => *tool,
        }
    }

    /// A list whose page size bounds the candidates captured, so clasify
    /// caps fan-out before the read runs. Symbols and references group rows
    /// by file; capping rows would cut one file's outline, so those pages are
    /// bounded by the cell check instead.
    pub(crate) fn is_paged_list(&self) -> bool {
        match self {
            Self::AstSearch(query) => matches!(
                **query,
                AstSearchQuery::MatchPattern(_) | AstSearchQuery::MatchRule(_)
            ),
            Self::GhSearchHistory(_) | Self::Other(ToolId::GhSearchRepo) => true,
            Self::ArtifactSearch(query) => query.package_name().is_none(),
            _ => false,
        }
    }

    /// A directory or file symbols outline.
    pub(crate) fn is_symbols(&self) -> bool {
        matches!(self, Self::AstSearch(query) if matches!(**query, AstSearchQuery::Symbols(_)))
    }

    /// The `owner`/`repo` a repo-scoped code search names.
    pub(crate) fn code_search_repository(&self) -> (Option<&str>, Option<&str>) {
        match self {
            Self::GhSearchCode(query) => (
                Some(query.owner.as_str()),
                query.repo.as_deref().map(String::as_str),
            ),
            _ => (None, None),
        }
    }
}

fn parse<T: DeserializeOwned>(prepared: Value) -> Option<Box<T>> {
    serde_json::from_value(prepared).ok().map(Box::new)
}
