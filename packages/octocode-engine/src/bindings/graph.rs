use crate::bindings::tasks::GraphFactsScanTask;
use crate::types::GraphFactsScanOptions;
use napi::bindgen_prelude::AsyncTask;
use napi_derive::napi;

/// Scan graph-fact-capable files, read them, and extract per-file facts in one
/// worker-pool operation. Cross-file policy and agent-facing output stay in
/// tools-core.
#[napi(js_name = "scanGraphFacts")]
pub fn scan_graph_facts(options: GraphFactsScanOptions) -> AsyncTask<GraphFactsScanTask> {
    AsyncTask::new(GraphFactsScanTask {
        options: Some(options),
    })
}
