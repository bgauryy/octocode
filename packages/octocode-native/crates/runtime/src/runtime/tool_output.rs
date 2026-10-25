//! The output-hook dispatch: each `ToolId` names the [`ToolOutput`] its tool
//! module implements. It lives with dispatch, so shared tool code never
//! imports a tool module.
use crate::id::ToolId;
use crate::response::output::ToolOutput;

impl ToolId {
    /// The output facts this tool owns.
    #[must_use]
    pub(crate) fn output(self) -> &'static dyn ToolOutput {
        match self {
            Self::GraphBuild | Self::GraphQuery => &crate::tools::ast_graph::store::tools::Output,
            Self::LocalSearch => &crate::tools::local_search::Output,
            Self::LocalFetch => &crate::tools::local_fetch::Output,
            Self::StructureSearch => &crate::tools::structure_search::Output,
            Self::AstSearch => &crate::tools::ast_search::Output,
            Self::LspSearch => &crate::tools::lsp_search::Output,
            Self::GhSearchRepo => &crate::tools::gh_search_repo::Output,
            Self::GhSearchCode => &crate::tools::gh_search_code::Output,
            Self::GhStructure => &crate::tools::gh_structure::Output,
            Self::GhGetFileContent => &crate::tools::gh_get_file_content::Output,
            Self::GhCloneRepo => &crate::tools::gh_clone_repo::Output,
            Self::GhSearchHistory => &crate::tools::gh_search_history::Output,
            Self::GhGetHistoryItem => &crate::tools::gh_get_history_item::Output,
            Self::ArtifactSearch => &crate::tools::artifact_search::Output,
            Self::Clasify => &crate::tools::clasify::Output,
        }
    }
}
