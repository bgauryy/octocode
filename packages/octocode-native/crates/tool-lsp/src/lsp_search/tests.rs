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
use crate::test_support::query;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::test_support::checks::location_rows;

mod call_counts;
mod call_rows;
mod caller_names;
mod caller_pages;
mod continuations;
mod diagnostics;
mod directory_roots;
mod empty_rows;
mod failures;
mod fixtures;
mod hierarchy;
mod hierarchy_caps;
mod limit;
#[cfg(unix)]
mod loading_server;
#[cfg(unix)]
mod missed_files;
mod operation_echo;
mod outline;
mod pagination;
mod receipts;
mod references;
mod requests;
mod revisions;
#[cfg(unix)]
mod row_quality;
#[cfg(unix)]
mod served_text;
mod server_context;
mod servers;
mod sources;
mod test_code;
mod unresolved_anchors;
#[cfg(unix)]
mod watching_server;

use fixtures::*;

/// A raw lspSearch row as the response layer publishes it: `status` and
/// other row-level fields lifted out of `data` (see `response::rows`).
fn public_row(data: &serde_json::Value) -> serde_json::Value {
    octocode_runtime_base::test_support::published(
        octocode_runtime_base::id::ToolId::LspSearch,
        data,
    )
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

/// `query` with `fields` layered over its row.
fn requery(query: &LspSearchQuery, fields: serde_json::Value) -> LspSearchQuery {
    let mut row = lsp_search_query::to_row(query);
    if let (Some(row), Some(fields)) = (row.as_object_mut(), fields.as_object()) {
        row.extend(fields.clone());
    }
    serde_json::from_value(row).expect("lsp query")
}

/// The workspace policy of the `/repo` fixtures.
fn repo_policy() -> octocode_runtime_base::policy::path::PathPolicy {
    octocode_runtime_base::test_support::workspace_policy(std::path::Path::new("/repo"))
}

/// Write an executable fixture (a stand-in language server) that a test then
/// starts. The bytes go to a temporary file that is closed and renamed into
/// place, so no write descriptor is open on the file when it is executed: a
/// concurrent fork that inherited one would make the start fail with
/// `ETXTBSY` ("Text file busy"). Every fixture executable is written here.
pub(super) fn write_executable(path: &std::path::Path, contents: impl AsRef<[u8]>) {
    octocode_io::atomic::write_atomic_mode(path, contents.as_ref(), 0o700, false)
        .expect("write an executable fixture");
}
