//! Unit tests for `lspSearch` (runtime side): anchors, coordinates,
//! pagination, source access, error mapping, and hierarchy walks over a
//! fake graph.

use super::anchor::*;
use super::failure::*;
use super::locations::*;
use super::ops::*;
use super::receipt::*;
use super::recovery::*;
use super::render::*;
use super::source::*;
use super::walk::*;
use super::*;
use octocode_contract_types::ErrorCode;

mod call_counts;
mod call_rows;
mod caller_pages;
mod continuations;
mod diagnostics;
mod empty_rows;
mod failures;
mod fixtures;
mod hierarchy;
mod hierarchy_caps;
mod outline;
mod pagination;
mod references;
mod requests;
mod server_context;
mod servers;
mod sources;
mod unresolved_anchors;

use fixtures::*;

/// A raw lspSearch row as the response layer publishes it: `status` and
/// other row-level fields lifted out of `data` (see `response::rows`).
fn public_row(data: &serde_json::Value) -> serde_json::Value {
    let row = crate::response::rows::result_row(
        crate::id::ToolId::LspSearch,
        0,
        &serde_json::json!({}),
        data.clone(),
        data.get("status").and_then(serde_json::Value::as_str),
    );
    let mut out = serde_json::json!({ "results": [row] });
    crate::response::continuations::finalize(
        &mut out,
        crate::id::ToolId::LspSearch,
        &crate::response::continuations::Sources::Rows(&[]),
        &crate::response::continuations::Scope::everything(),
    )
    .expect("valid continuations");
    crate::response::channels::publish_hints(&mut out, crate::id::ToolId::LspSearch);
    out
}

/// `data` as the response stage finishes it for `query`: the tool's recovery
/// hint (`ToolOutput::error_hint` / `category_hint`) placed beside its leads.
fn with_stage_hints(query: &LspSearchQuery, data: &serde_json::Value) -> serde_json::Value {
    let mut row = serde_json::json!({
        "status": data.get("status").cloned().unwrap_or_default(),
        "data": data,
    });
    crate::response::rows::apply_hint_policy(
        &mut row,
        crate::id::ToolId::LspSearch,
        &lsp_search_query::to_row(query),
    );
    row["data"].take()
}

/// Client options for fake servers, which answer at once: no settle wait.
pub(super) const FAKE_SERVER_CLIENT: octocode_engine::lsp::client::LspClientOptions =
    octocode_engine::lsp::client::LspClientOptions {
        settle_cap: Some(std::time::Duration::ZERO),
    };

/// A pool whose clients take [`FAKE_SERVER_CLIENT`].
pub(super) fn fake_server_pool() -> octocode_engine::lsp::pool::LspClientPool {
    octocode_engine::lsp::pool::LspClientPool::new(octocode_engine::lsp::pool::LspPoolOptions {
        client: FAKE_SERVER_CLIENT,
        ..octocode_engine::lsp::pool::LspPoolOptions::default()
    })
}

/// A page budget large enough that only an explicit count cuts a test page.
const TEST_BUDGET: usize = 16_000;

fn query(value: serde_json::Value) -> LspSearchQuery {
    serde_json::from_value(value).expect("lsp query")
}

/// `query` with `fields` layered over its row.
fn requery(query: &LspSearchQuery, fields: serde_json::Value) -> LspSearchQuery {
    let mut row = lsp_search_query::to_row(query);
    if let (Some(row), Some(fields)) = (row.as_object_mut(), fields.as_object()) {
        row.extend(fields.clone());
    }
    serde_json::from_value(row).expect("lsp query")
}

/// The workspace policy of the `/repo` fixtures.
fn repo_policy() -> crate::policy::path::PathPolicy {
    crate::tools::test_support::workspace_policy(std::path::Path::new("/repo"))
}
