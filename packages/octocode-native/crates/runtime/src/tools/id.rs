//! Canonical tool identity.
//!
//! Single source of truth for the runtime's tool-name strings and the family
//! classification used by admission (`is_available`), cursor scoping
//! (`cursor_scope_for`), and dispatch. Before this module the same family
//! lists were re-spelled as string `matches!` arms in several places; keeping
//! one typed enum here prevents those copies from drifting apart.

use std::str::FromStr;

/// Family used to partition cursor scope digests and remote/local policy.
///
/// Toggling local-only config (e.g. `enable_clone`) must not invalidate
/// GitHub/remote cursors, so each family scopes its digest differently.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolFamily {
    /// Local-filesystem tools: scoped by cwd, allowed paths, and LSP config.
    Local,
    /// GitHub API tools: scoped by API endpoint + home, not local config.
    GitHub,
    /// Other remote tools (artifact, jev): scoped by home only.
    Remote,
}

/// Every tool the native runtime can execute — the twelve public tools plus
/// the internal `jevScout` reasoning sub-tool.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ToolId {
    GhSearch,
    GhGetFileContent,
    GhSearchHistory,
    GhGetHistoryItem,
    GhCloneRepo,
    ArtifactSearch,
    LocalSearch,
    LocalFetch,
    AstSearch,
    AstRewrite,
    LspSearch,
    JevReasoning,
    JevScout,
}

impl ToolId {
    /// All tool identities in declaration order. Public tools plus `JevScout`.
    pub const ALL: [ToolId; 13] = [
        ToolId::GhSearch,
        ToolId::GhGetFileContent,
        ToolId::GhSearchHistory,
        ToolId::GhGetHistoryItem,
        ToolId::GhCloneRepo,
        ToolId::ArtifactSearch,
        ToolId::LocalSearch,
        ToolId::LocalFetch,
        ToolId::AstSearch,
        ToolId::AstRewrite,
        ToolId::LspSearch,
        ToolId::JevReasoning,
        ToolId::JevScout,
    ];

    /// The wire name exactly as it appears in the generated contract and in
    /// tool-call envelopes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ToolId::GhSearch => "ghSearch",
            ToolId::GhGetFileContent => "ghGetFileContent",
            ToolId::GhSearchHistory => "ghSearchHistory",
            ToolId::GhGetHistoryItem => "ghGetHistoryItem",
            ToolId::GhCloneRepo => "ghCloneRepo",
            ToolId::ArtifactSearch => "artifactSearch",
            ToolId::LocalSearch => "localSearch",
            ToolId::LocalFetch => "localFetch",
            ToolId::AstSearch => "astSearch",
            ToolId::AstRewrite => "astRewrite",
            ToolId::LspSearch => "lspSearch",
            ToolId::JevReasoning => "jevReasoning",
            ToolId::JevScout => "jevScout",
        }
    }

    /// Resolve a wire name to its identity, or `None` for an unknown tool.
    #[must_use]
    pub fn from_name(name: &str) -> Option<ToolId> {
        ToolId::ALL.into_iter().find(|id| id.as_str() == name)
    }

    /// Cursor-scope / policy family this tool belongs to.
    #[must_use]
    pub const fn family(self) -> ToolFamily {
        match self {
            ToolId::LocalSearch
            | ToolId::LocalFetch
            | ToolId::AstSearch
            | ToolId::AstRewrite
            | ToolId::LspSearch => ToolFamily::Local,
            ToolId::GhSearch
            | ToolId::GhGetFileContent
            | ToolId::GhSearchHistory
            | ToolId::GhGetHistoryItem
            | ToolId::GhCloneRepo => ToolFamily::GitHub,
            ToolId::ArtifactSearch | ToolId::JevReasoning | ToolId::JevScout => ToolFamily::Remote,
        }
    }

    /// A local-filesystem tool (honours local policy + allowed paths).
    #[must_use]
    pub const fn is_local(self) -> bool {
        matches!(self.family(), ToolFamily::Local)
    }

    /// A GitHub API tool (includes `ghCloneRepo`; clone has an extra gate).
    #[must_use]
    pub const fn is_github(self) -> bool {
        matches!(self.family(), ToolFamily::GitHub)
    }

    /// A Jev reasoning tool (gated on a non-blank `OCTOCODE_JEV_KEY`).
    #[must_use]
    pub const fn is_jev(self) -> bool {
        matches!(self, ToolId::JevReasoning | ToolId::JevScout)
    }
}

impl FromStr for ToolId {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        ToolId::from_name(value).ok_or(())
    }
}

impl std::fmt::Display for ToolId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for id in ToolId::ALL {
            assert!(seen.insert(id.as_str()), "duplicate name {}", id.as_str());
            assert_eq!(ToolId::from_name(id.as_str()), Some(id));
            assert_eq!(id.as_str().parse::<ToolId>(), Ok(id));
        }
        assert_eq!(ToolId::from_name("nope"), None);
        assert!("nope".parse::<ToolId>().is_err());
    }

    #[test]
    fn every_tool_has_exactly_one_family() {
        for id in ToolId::ALL {
            let count = [id.is_local(), id.is_github(), id.is_jev()]
                .into_iter()
                .filter(|flag| *flag)
                .count();
            // Jev tools are Remote family, so is_jev is orthogonal; assert the
            // Local/GitHub families are mutually exclusive and cover no jev tool.
            if id.is_jev() {
                assert!(!id.is_local() && !id.is_github());
                assert_eq!(id.family(), ToolFamily::Remote);
            } else {
                assert!(count <= 1, "{} matched multiple families", id.as_str());
            }
        }
    }
}
