//! The surface a call comes from, which decides the runtime that answers it
//! and the reply it gets.

use octocode_native::runtime::HostOptions;
use octocode_runtime_base::config::RuntimeSurface;

/// Who asked: the CLI (a call answered as printed) or an MCP server (a call
/// answered as an MCP result). Each is a runtime of its own on a server,
/// because the surface decides tool availability, policy and the keys of
/// page-replay cursors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(in crate::cli) enum Surface {
    Cli,
    Mcp,
}

impl Surface {
    /// The surface's name in a request.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Mcp => "mcp",
        }
    }

    pub(super) fn parse(name: &str) -> Option<Self> {
        [Self::Cli, Self::Mcp]
            .into_iter()
            .find(|surface| surface.as_str() == name)
    }

    /// The configuration surface of a runtime for this one.
    pub(super) const fn runtime(self) -> RuntimeSurface {
        match self {
            Self::Cli => RuntimeSurface::Cli,
            Self::Mcp => RuntimeSurface::Mcp,
        }
    }

    /// The host a runtime for this surface starts from: the one an
    /// in-process call of the surface uses.
    pub(super) fn host(self) -> HostOptions {
        match self {
            Self::Cli => crate::cli::cli_host(),
            Self::Mcp => crate::cli::mcp_server::mcp_host(),
        }
    }
}
