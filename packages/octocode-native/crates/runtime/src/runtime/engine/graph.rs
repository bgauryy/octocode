use super::ToolRuntime;
use crate::config::{self};
use crate::tools::ast_graph::store;
use octocode_contract_types::ErrorCode;
use std::path::PathBuf;

impl ToolRuntime {
    /// CLI `graph ingest`: builds and publishes a code-graph snapshot under
    /// the same path policy and content security as the local tools.
    pub fn graph_ingest(&self, options: &store::IngestOptions) -> store::GraphOutput {
        if !self.config.resolved.local.enabled {
            return graph_local_disabled();
        }
        let options = store::IngestOptions {
            workspace: Some(self.graph_workspace(options.workspace.as_deref())),
            ..options.clone()
        };
        self.seal_graph(store::ingest(
            &options,
            &self.paths,
            &self.security,
            &crate::execution::cancel::NeverCancel,
            self.config.env_value(config::environment::OCTOCODE_CARGO),
        ))
    }

    /// CLI `graph query`: answers one bounded question from a snapshot.
    pub fn graph_query(&self, options: &store::QueryOptions) -> store::GraphOutput {
        if !self.config.resolved.local.enabled {
            return graph_local_disabled();
        }
        let options = store::QueryOptions {
            workspace: Some(self.graph_workspace(options.workspace.as_deref())),
            ..options.clone()
        };
        self.seal_graph(store::query(&options, &self.paths))
    }

    /// A graph answer through the same output gate as every tool result:
    /// it carries engine data and echoed input.
    fn seal_graph(&self, output: store::GraphOutput) -> store::GraphOutput {
        let value = self
            .security
            .seal_output(
                output.value,
                false,
                || Ok::<_, std::convert::Infallible>(()),
            )
            .unwrap_or_else(|never| match never {});
        store::GraphOutput {
            value: value.into_value(),
            ..output
        }
    }

    /// The workspace that owns a graph: `explicit`, else the one this
    /// runtime's working directory belongs to (not the process's).
    pub(super) fn graph_workspace(&self, explicit: Option<&std::path::Path>) -> PathBuf {
        explicit.map_or_else(
            || store::default_workspace(self.input.cwd.clone(), &self.paths),
            std::path::Path::to_path_buf,
        )
    }
}

/// `graph` reads local files, so it runs only where local tools do.
fn graph_local_disabled() -> store::GraphOutput {
    store::GraphOutput::error(
        ErrorCode::MissingConfiguration,
        format!(
            "Local tools are disabled; graph commands read local files. Set {}.",
            config::field::LOCAL_ENABLED.display_name()
        ),
    )
}
