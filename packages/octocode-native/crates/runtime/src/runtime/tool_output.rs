//! The output-hook dispatch: each `ToolId` names the [`ToolOutput`] its tool
//! module implements. It lives with dispatch, which hands it to the shared
//! response stages, so neither they nor shared tool code import a tool module.
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::output::{SharedOutput, ToolOutput};

/// The tool module's output hooks of a [`ToolId`]. A trait, not an inherent
/// method: `ToolId` lives in `octocode-runtime-base`, below the tools, while
/// each tool's [`ToolOutput`] lives here with the tool, and only the crate
/// that defines a type may give it inherent methods. Callers import it.
pub(crate) trait ToolOutputOf {
    /// The recovery and selection finish this tool owns.
    fn output(self) -> &'static dyn ToolOutput;
}

impl ToolOutputOf for ToolId {
    fn output(self) -> &'static dyn ToolOutput {
        match self {
            Self::LocalSearch => &octocode_tool_local::local_search::Output,
            Self::LocalFetch => &octocode_tool_local::local_fetch::Output,
            Self::AstSearch => &octocode_tool_ast::ast_search::Output,
            Self::LspSearch => &octocode_tool_lsp::lsp_search::Output,
            Self::GhSearchRepo => &octocode_tool_github::gh_search_repo::Output,
            Self::GhSearchCode => &octocode_tool_github::gh_search_code::Output,
            Self::GhGetFileContent => &octocode_tool_github::gh_get_file_content::Output,
            Self::GhGetHistoryItem => &octocode_tool_github::gh_get_history_item::Output,
            Self::GhStructure => &octocode_tool_github::gh_structure::Output,
            Self::GraphBuild | Self::GraphQuery => &octocode_tool_graph::ast_graph::Output,
            Self::SecurityCheck => &octocode_tool_packages::security_check::Output,
            Self::StructureSearch => &octocode_tool_local::structure_search::Output,
            Self::GhSearchHistory => &octocode_tool_github::gh_search_history::Output,
            Self::ArtifactSearch => &octocode_tool_packages::artifact_search::Output,
            Self::GhCloneRepo | Self::Clasify => &SharedOutput,
        }
    }
}

#[cfg(test)]
mod tests;
