use crate::bindings::tasks::FileSystemQueryTask;
use crate::types::FileSystemQueryOptions;
use napi::bindgen_prelude::AsyncTask;
use napi_derive::napi;

/// Cross-platform filesystem traversal and metadata filtering for local tools.
///
/// Replaces the POSIX `find`/`ls` execution paths in octocode-tools-core while
/// keeping MCP response shaping in TypeScript.
#[napi(js_name = "queryFileSystem")]
pub fn query_file_system(options: FileSystemQueryOptions) -> AsyncTask<FileSystemQueryTask> {
    AsyncTask::new(FileSystemQueryTask {
        options: Some(options),
    })
}
