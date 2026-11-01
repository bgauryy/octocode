//! The output-hook dispatch: each `ToolId` names the [`ToolOutput`] its tool
//! module implements. It lives with dispatch, which hands it to the shared
//! response stages, so neither they nor shared tool code import a tool module.
use crate::id::ToolId;
use crate::response::output::{SharedOutput, ToolOutput};

impl ToolId {
    /// The recovery and selection finish this tool owns.
    #[must_use]
    pub(crate) fn output(self) -> &'static dyn ToolOutput {
        match self {
            Self::LocalSearch => &crate::tools::local_search::Output,
            Self::LocalFetch => &crate::tools::local_fetch::Output,
            Self::AstSearch => &crate::tools::ast_search::Output,
            Self::LspSearch => &crate::tools::lsp_search::Output,
            Self::GhSearchRepo => &crate::tools::gh_search_repo::Output,
            Self::GhSearchCode => &crate::tools::gh_search_code::Output,
            Self::GhGetFileContent => &crate::tools::gh_get_file_content::Output,
            Self::GhGetHistoryItem => &crate::tools::gh_get_history_item::Output,
            Self::GraphBuild
            | Self::GraphQuery
            | Self::StructureSearch
            | Self::GhStructure
            | Self::GhCloneRepo
            | Self::GhSearchHistory
            | Self::ArtifactSearch
            | Self::Clasify => &SharedOutput,
        }
    }
}
