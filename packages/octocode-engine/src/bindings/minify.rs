use crate::bindings::tasks::MinifyContentTask;
use napi::bindgen_prelude::AsyncTask;
use napi_derive::napi;

/// Full minification on libuv's worker pool.
/// Returns a Promise from JavaScript and does not block the event loop.
#[napi(js_name = "minifyContent")]
pub fn minify_content(content: String, file_path: String) -> AsyncTask<MinifyContentTask> {
    AsyncTask::new(MinifyContentTask { content, file_path })
}

/// Agent-readable "standard" view: strips comments and blank-line noise while
/// preserving indentation and code shape. Capped at 1MB; panic-contained.
#[napi(js_name = "applyContentViewMinification")]
pub fn apply_content_view_minification(content: String, file_path: String) -> String {
    crate::signatures::run_on_deep_stack(move || {
        crate::minify::apply::apply_content_view_minification_inner(&content, &file_path)
    })
}
