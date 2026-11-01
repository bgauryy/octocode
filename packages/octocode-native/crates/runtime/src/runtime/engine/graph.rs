use super::ToolRuntime;
use crate::tools::ast_graph::store;
use std::path::PathBuf;

impl ToolRuntime {
    /// The workspace that owns a graph: `explicit`, else the one this
    /// runtime's working directory belongs to (not the process's).
    pub(super) fn graph_workspace(&self, explicit: Option<&std::path::Path>) -> PathBuf {
        explicit.map_or_else(
            || store::default_workspace(self.input.cwd.clone(), &self.paths),
            std::path::Path::to_path_buf,
        )
    }
}
