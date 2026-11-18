//! ghGetHistoryItem facts as an agent receives them, against a mock GitHub:
//! what closed an issue and why, a discussion in GitHub's order, a review
//! folded into its inline comments, a comparison's missing ref named, and
//! patch windows stated once.

mod compare;
mod issues;
mod patches;
mod pull_requests;

use crate::support::{Workspace, call};
use octocode_native::runtime::ToolRuntime;
use serde_json::Value;
use wiremock::MockServer;

/// One runtime against a mock GitHub, so a walk's cursors resolve.
pub(crate) struct Session {
    _workspace: Workspace,
    runtime: ToolRuntime,
}

impl Session {
    pub(crate) fn new(server: &MockServer) -> Self {
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
        Self {
            _workspace: workspace,
            runtime,
        }
    }

    /// One ghGetHistoryItem call, as the serialized response an agent
    /// reads, parsed back.
    pub(crate) async fn served(&self, query: Value) -> Value {
        let outcome = call(&self.runtime, "ghGetHistoryItem", query)
            .await
            .expect("ghGetHistoryItem");
        serde_json::from_str(&outcome.structured_content.to_string()).expect("JSON")
    }

    pub(crate) async fn close(self) {
        self.runtime.close().await;
    }
}

/// One ghGetHistoryItem call against `server` in a runtime of its own.
pub(crate) async fn served(server: &MockServer, query: Value) -> Value {
    let session = Session::new(server);
    let served = session.served(query).await;
    session.close().await;
    served
}

/// The first result row's `data` of a served response.
pub(crate) fn data(served: &Value) -> &Value {
    &served["results"][0]["data"]
}

/// The related calls of a served row to `tool`.
pub(crate) fn related<'a>(data: &'a Value, tool: &str) -> Vec<&'a Value> {
    data["related"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|call| call["tool"] == tool)
        .collect()
}
