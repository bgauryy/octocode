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

/// Every tool the native runtime can execute.
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
    SemanticAssess,
}

impl ToolId {
    /// All tool identities in declaration order.
    pub const ALL: [ToolId; 12] = [
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
        ToolId::SemanticAssess,
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
            ToolId::SemanticAssess => "semanticAssess",
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
            ToolId::ArtifactSearch | ToolId::SemanticAssess => ToolFamily::Remote,
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

    /// A semantic assessment tool (gated on a non-blank `OCTOCODE_CLASSIFICATION_API`
    /// or the selected vendor's native key, e.g. `OCTOCODE_JEV_KEY`).
    #[must_use]
    pub const fn is_semantic_assess(self) -> bool {
        matches!(self, ToolId::SemanticAssess)
    }

    /// Env-var hint shown in the CLI `scheme` catalog when a tool is disabled.
    /// `None` means availability is controlled via `tools.enabled`/`disabled`.
    /// `ENABLE_CLONE` and `OCTOCODE_ENABLE_CLONE` are accepted aliases
    /// (resolver.rs); only the canonical short form is shown here.
    #[must_use]
    pub const fn availability_env_hint(self) -> Option<&'static str> {
        match self {
            ToolId::GhCloneRepo => Some("ENABLE_CLONE|OCTOCODE_STORAGE_MODE"),
            ToolId::SemanticAssess => Some("OCTOCODE_CLASSIFICATION_API|OCTOCODE_JEV_KEY"),
            ToolId::AstRewrite => Some("ENABLE_AST_REWRITE"),
            ToolId::LocalSearch | ToolId::LocalFetch | ToolId::AstSearch | ToolId::LspSearch => {
                Some("ENABLE_LOCAL")
            }
            _ => None,
        }
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
    fn family_covers_every_tool() {
        // GitHub tools
        for id in [
            ToolId::GhSearch,
            ToolId::GhGetFileContent,
            ToolId::GhSearchHistory,
            ToolId::GhGetHistoryItem,
            ToolId::GhCloneRepo,
        ] {
            assert!(id.is_github(), "{id}");
        }
        // Local tools
        for id in [
            ToolId::LocalSearch,
            ToolId::LocalFetch,
            ToolId::AstSearch,
            ToolId::AstRewrite,
            ToolId::LspSearch,
        ] {
            assert!(id.is_local(), "{id}");
        }
        assert!(ToolId::SemanticAssess.is_semantic_assess());
        assert_eq!(ToolId::from_name("jev"), None);
    }

    #[test]
    fn availability_env_hint_covers_gated_tools_and_is_none_for_always_on() {
        // Always available (no env gate)
        for id in [
            ToolId::GhSearch,
            ToolId::GhGetFileContent,
            ToolId::GhSearchHistory,
            ToolId::GhGetHistoryItem,
            ToolId::ArtifactSearch,
        ] {
            assert!(
                id.availability_env_hint().is_none(),
                "{id} should have no hint"
            );
        }
        // Env-gated tools
        assert_eq!(
            ToolId::GhCloneRepo.availability_env_hint(),
            Some("ENABLE_CLONE|OCTOCODE_STORAGE_MODE")
        );
        assert_eq!(
            ToolId::SemanticAssess.availability_env_hint(),
            Some("OCTOCODE_CLASSIFICATION_API|OCTOCODE_JEV_KEY")
        );
        for id in [
            ToolId::LocalSearch,
            ToolId::LocalFetch,
            ToolId::AstSearch,
            ToolId::LspSearch,
        ] {
            assert_eq!(id.availability_env_hint(), Some("ENABLE_LOCAL"), "{id}");
        }
        assert_eq!(
            ToolId::AstRewrite.availability_env_hint(),
            Some("ENABLE_AST_REWRITE")
        );
    }

    #[test]
    fn every_tool_has_exactly_one_family() {
        for id in ToolId::ALL {
            let count = [id.is_local(), id.is_github(), id.is_semantic_assess()]
                .into_iter()
                .filter(|flag| *flag)
                .count();
            // Jev tools are Remote family, so is_jev is orthogonal; assert the
            // Local/GitHub families are mutually exclusive and cover no jev tool.
            if id.is_semantic_assess() {
                assert!(!id.is_local() && !id.is_github());
                assert_eq!(id.family(), ToolFamily::Remote);
            } else {
                assert!(count <= 1, "{} matched multiple families", id.as_str());
            }
        }
    }
}
