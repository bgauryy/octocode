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
/// Toggling local-only config (e.g. `allowed_paths`) must not invalidate
/// GitHub/remote cursors, so each family scopes its digest differently.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolFamily {
    /// Local-filesystem tools: scoped by cwd, allowed paths, and LSP config.
    Local,
    /// GitHub API tools: scoped by API endpoint + home, not local config.
    GitHub,
    /// Other remote tools (artifact, clasify): scoped by home only.
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
    AstTopology,
    AstRewrite,
    LspSearch,
    Clasify,
}

impl ToolId {
    /// All tool identities in declaration order.
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
        ToolId::AstTopology,
        ToolId::AstRewrite,
        ToolId::LspSearch,
        ToolId::Clasify,
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
            ToolId::AstTopology => "astTopology",
            ToolId::AstRewrite => "astRewrite",
            ToolId::LspSearch => "lspSearch",
            ToolId::Clasify => "clasify",
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
            | ToolId::AstTopology
            | ToolId::AstRewrite
            | ToolId::LspSearch => ToolFamily::Local,
            ToolId::GhSearch
            | ToolId::GhGetFileContent
            | ToolId::GhSearchHistory
            | ToolId::GhGetHistoryItem
            | ToolId::GhCloneRepo => ToolFamily::GitHub,
            ToolId::ArtifactSearch | ToolId::Clasify => ToolFamily::Remote,
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
    pub const fn is_clasify(self) -> bool {
        matches!(self, ToolId::Clasify)
    }

    /// Independent read-only queries may execute concurrently. Mutating clone
    /// and rewrite operations remain ordered; clasify owns its own bounded
    /// provider scheduler.
    #[must_use]
    pub const fn supports_concurrent_queries(self) -> bool {
        !matches!(
            self,
            ToolId::GhCloneRepo | ToolId::AstRewrite | ToolId::Clasify
        )
    }

    /// Tools hidden from discovery unless the shared beta gate is enabled.
    #[must_use]
    pub const fn is_beta(self) -> bool {
        matches!(self, ToolId::AstRewrite | ToolId::AstTopology)
    }

    /// Env-var hint shown in the CLI `scheme` catalog when a tool is disabled.
    /// `None` means availability is controlled via `tools.enabled`/`disabled`.
    /// Clone is CLI-only and requires persistent storage; MCP never exposes it.
    #[must_use]
    pub const fn availability_env_hint(self) -> Option<&'static str> {
        match self {
            ToolId::GhCloneRepo => Some("OCTOCODE_STORAGE_MODE"),
            ToolId::Clasify => Some("OCTOCODE_CLASSIFICATION_API|OCTOCODE_JEV_KEY"),
            ToolId::AstRewrite | ToolId::AstTopology => Some("OCTOCODE_BETA"),
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
        // Hard cutover: the pre-rename public name no longer resolves to a tool.
        assert_eq!(ToolId::from_name("semanticAssess"), None);
        assert!("semanticAssess".parse::<ToolId>().is_err());
    }

    #[test]
    fn concurrent_queries_exclude_mutating_and_self_scheduled_tools() {
        for id in ToolId::ALL {
            assert_eq!(
                id.supports_concurrent_queries(),
                !matches!(
                    id,
                    ToolId::GhCloneRepo | ToolId::AstRewrite | ToolId::Clasify
                ),
                "{id}"
            );
        }
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
            ToolId::AstTopology,
            ToolId::AstRewrite,
            ToolId::LspSearch,
        ] {
            assert!(id.is_local(), "{id}");
        }
        assert!(ToolId::Clasify.is_clasify());
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
            Some("OCTOCODE_STORAGE_MODE")
        );
        assert_eq!(
            ToolId::Clasify.availability_env_hint(),
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
        for id in [ToolId::AstRewrite, ToolId::AstTopology] {
            assert!(id.is_beta());
            assert_eq!(id.availability_env_hint(), Some("OCTOCODE_BETA"));
        }
    }

    #[test]
    fn every_tool_has_exactly_one_family() {
        for id in ToolId::ALL {
            let count = [id.is_local(), id.is_github(), id.is_clasify()]
                .into_iter()
                .filter(|flag| *flag)
                .count();
            // Clasify is Remote family, so is_clasify is orthogonal; assert the
            // Local/GitHub families are mutually exclusive and cover no clasify tool.
            if id.is_clasify() {
                assert!(!id.is_local() && !id.is_github());
                assert_eq!(id.family(), ToolFamily::Remote);
            } else {
                assert!(count <= 1, "{} matched multiple families", id.as_str());
            }
        }
    }
}
