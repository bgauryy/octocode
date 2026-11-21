//! Incoming calls keep call sites only. A server may answer `incomingCalls`
//! from the symbol's references (rust-analyzer), so a function-local
//! `use a::{f}` arrives as a site of the function around it: an import
//! binding, not a call.

use crate::lsp_search::failure::LspFailure;
use crate::lsp_search::render::uri_to_path;
use crate::lsp_search::roles::import_sites;
use crate::lsp_search::source::SourceCache;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::policy::path::PathPolicy;
use serde_json::{Value, json};

/// Drop the import bindings from each incoming call's `fromRanges`, and a
/// caller whose every site was one: it imports the symbol and never calls
/// it. A caller the server gave no site for, or whose file cannot be read
/// here, stays as the server answered.
pub(super) async fn drop_import_sites(
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    results: &mut Vec<Value>,
) -> Result<(), LspFailure> {
    let mut sources = SourceCache::new(paths);
    let mut kept = Vec::with_capacity(results.len());
    for mut result in std::mem::take(results) {
        let path = result
            .pointer("/from/uri")
            .and_then(Value::as_str)
            .map(uri_to_path);
        let sites = result
            .get("fromRanges")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let starts = sites.iter().filter_map(start).collect::<Vec<_>>();
        if let Some(path) = path
            && !sites.is_empty()
            && starts.len() == sites.len()
            && let Some(imports) = import_sites(&mut sources, cancel, &path, &starts).await?
        {
            let calls = sites
                .into_iter()
                .zip(imports)
                .filter(|(_, import)| !import)
                .map(|(site, _)| site)
                .collect::<Vec<_>>();
            if calls.is_empty() {
                continue;
            }
            result["fromRanges"] = json!(calls);
        }
        kept.push(result);
    }
    *results = kept;
    Ok(())
}

/// The zero-based `(line, character)` a raw site range starts at.
fn start(site: &Value) -> Option<(u32, u32)> {
    let point = |field: &str| {
        site.pointer(&format!("/start/{field}"))
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
    };
    Some((point("line")?, point("character")?))
}
