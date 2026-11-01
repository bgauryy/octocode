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
use crate::id::ToolId;
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
mod limit;
mod outline;
mod pagination;
mod receipts;
mod references;
mod requests;
mod server_context;
mod servers;
mod sources;
mod test_code;
mod unresolved_anchors;

use fixtures::*;

/// A raw lspSearch row as the response layer publishes it: `status` and
/// other row-level fields lifted out of `data` (see `response::rows`).
fn public_row(data: &serde_json::Value) -> serde_json::Value {
    crate::tools::test_support::published(crate::id::ToolId::LspSearch, data)
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
