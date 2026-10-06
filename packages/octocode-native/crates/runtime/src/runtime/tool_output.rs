//! The output-hook dispatch: each `ToolId` names the [`ToolOutput`] its tool
//! module implements. It lives with dispatch, so shared tool code never
//! imports a tool module.
use crate::tools::id::ToolId;
use crate::tools::output::ToolOutput;

impl ToolId {
    /// The output facts this tool owns.
    #[must_use]
    pub fn output(self) -> &'static dyn ToolOutput {
        match self {
            ToolId::LocalSearch => &crate::tools::local_search::Output,
            ToolId::LocalFetch => &crate::tools::local_fetch::Output,
            ToolId::StructureSearch => &crate::tools::structure_search::Output,
            ToolId::AstSearch => &crate::tools::ast_search::Output,
            ToolId::LspSearch => &crate::tools::lsp_search::Output,
            ToolId::AstTopology => &crate::tools::ast_graph::Output,
            ToolId::AstRewrite => &crate::tools::ast_rewrite::Output,
            ToolId::GhSearchRepo => &crate::tools::gh_search_repo::Output,
            ToolId::GhSearchCode => &crate::tools::gh_search_code::Output,
            ToolId::GhStructure => &crate::tools::gh_structure::Output,
            ToolId::GhGetFileContent => &crate::tools::gh_get_file_content::Output,
            ToolId::GhCloneRepo => &crate::tools::gh_clone_repo::Output,
            ToolId::GhSearchHistory => &crate::tools::gh_search_history::Output,
            ToolId::GhGetHistoryItem => &crate::tools::gh_get_history_item::Output,
            ToolId::ArtifactSearch => &crate::tools::artifact_search::Output,
            ToolId::Clasify => &crate::tools::clasify::Output,
        }
    }
}
