//! Saved response pages: the settled envelope of a paged response, kept so
//! a later page replays it without re-executing the batch, and the saved
//! read that opens one.

use super::RuntimeError;
use crate::config::{self, ConfigOutput};
use crate::response::cursor::{Cursor, ROW_PLAN_VERSION};
use crate::response::pager::SavedPages;
use crate::response::result::FailureKind;
use crate::security::{CheckedQuery, Sanitized};
use octocode_contract_types::ErrorCode;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// How long saved work outlives its last use: every read of a saved
/// response or an issued handle restarts it ([`crate::cache::CacheClass::Idle`]),
/// so a walk stays alive while its pages are read.
pub(super) const RETENTION_IDLE: Duration = Duration::from_secs(600);

/// The lifetime class of saved responses and issued handles.
pub(super) const RETENTION: crate::cache::CacheClass = crate::cache::CacheClass::Idle {
    ttl: RETENTION_IDLE,
};

/// Immutable settled output. Page reads never consult its original sources.
#[derive(Serialize, Deserialize)]
pub(super) struct PageReplay {
    pub(super) access: [u8; 32],
    pub(super) github_identity: Option<String>,
    /// Sealed: a store reloaded from disk passes the output gate again.
    pub(super) structured: Sanitized,
    pub(super) remaining: Vec<Vec<crate::response::result::Remaining>>,
    pub(super) response_query: Value,
    pub(super) failure: Option<FailureKind>,
    /// The text and page source later reads cut from (rebuilt from
    /// `structured` after a reload from disk).
    #[serde(skip)]
    pub(super) pages: SavedPages,
}

pub(super) struct PageReplayMemo {
    pub(super) store: crate::cache::Store<PageReplay>,
    pub(super) policy: String,
    pub(super) persistent: bool,
}

impl PageReplayMemo {
    pub(super) fn cache_config() -> crate::cache::CacheConfig {
        // No entry count: the byte budgets alone bound retention, so a burst
        // of small saved responses or handles never evicts an active walk.
        crate::cache::CacheConfig {
            max_entries: usize::MAX,
            max_bytes: 32 * 1024 * 1024,
            disk_bytes: 128 * 1024 * 1024,
        }
    }

    pub(super) fn new(config: &ConfigOutput, cwd: &std::path::Path) -> Self {
        let persistent = config::is_persistent_storage_enabled(&config.resolved);
        let policy = replay_policy(config, cwd, ROW_PLAN_VERSION);
        Self {
            store: crate::cache::Store::new(
                Self::cache_config(),
                persistent.then(|| crate::cache::Directory::ResponsePages.path(&config.home)),
            ),
            policy,
            persistent,
        }
    }

    /// A saved response is stored under its cursor's page-independent
    /// name: one entry per content, scope and page length.
    fn key(cursor: &Cursor, policy: &str) -> crate::cache::CacheKey {
        crate::cache::CacheKey {
            namespace: "response-pages-v2".into(),
            scope: format!("{policy}:{}", hex::encode(cursor.key)),
            resource: cursor.resource(),
        }
    }

    pub(super) fn get(&self, cursor: &Cursor) -> Option<Arc<PageReplay>> {
        self.store
            .get(&Self::key(cursor, &self.policy))
            .map(|hit| hit.value)
    }

    pub(super) fn insert(&self, cursor: &Cursor, replay: PageReplay) -> (Arc<PageReplay>, bool) {
        // The page memo a first read builds holds about one more copy of
        // the envelope (its row plan, bytes or text).
        let weight = octocode_engine::json_size::bytes(&replay)
            .map_or(usize::MAX, |bytes| bytes.saturating_mul(2));
        self.store
            .put_confirmed(Self::key(cursor, &self.policy), replay, weight, RETENTION)
    }
}

fn replay_policy(config: &ConfigOutput, cwd: &std::path::Path, plan_version: u32) -> String {
    octocode_engine::digest::sha256(
        format!(
            "{}:{}:{}:{plan_version}",
            config.identity_digest(),
            cwd.display(),
            octocode_contract_types::TOOL_TYPES_CONTRACT_FINGERPRINT
        )
        .as_bytes(),
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod refusal_tests;

pub(super) fn page_expiry() -> String {
    crate::civil_date::iso8601_millis_after(RETENTION_IDLE)
}

/// Why saved work does not answer a cursor. Each cause has its own text,
/// true on both surfaces and under either storage mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Refusal {
    /// Not the shape of an issued handle: edited, truncated or invented.
    Malformed,
    /// No saved work under the handle in this runtime's storage.
    NotRetained,
    /// Issued to another tool.
    OtherTool,
    /// Issued under another access identity: another request credential.
    OtherAccess,
    /// Issued on the other surface (CLI or MCP) under the same credential.
    OtherSurface,
    /// The current GitHub identity is not (or cannot be confirmed as) the
    /// one the handle was issued to.
    IdentityChanged,
    /// The saved request no longer admits to the rows its page was cut from.
    RequestChanged,
}

/// Where a refused cursor was sent: the storage and surface its text names.
#[derive(Clone, Copy)]
pub(super) struct RefusalScope {
    pub(super) persistent: bool,
    pub(super) mcp: bool,
}

/// The `responseSnapshotExpired` error for `refusal`: its cause, then the
/// recovery every refusal shares.
pub(super) fn snapshot_expired(refusal: Refusal, scope: RefusalScope) -> RuntimeError {
    let lifetime = RETENTION_IDLE.as_secs();
    let cause = match refusal {
        Refusal::Malformed => {
            "Malformed cursor: it is not an issued handle (edited, truncated or invented).".into()
        }
        Refusal::NotRetained if scope.persistent => format!(
            "Unknown or expired cursor: no saved work under it for this tool, configuration and workspace. Saved work expires {lifetime} seconds after its last read and may be evicted; an edited cursor, or one issued under another configuration or workspace, names none here."
        ),
        Refusal::NotRetained => format!(
            "Unknown or expired cursor: no saved work under it in this runtime. storage.mode is memory, so saved work ends with the runtime that issued it (each CLI call is its own runtime); set storage.mode persistent to resume across CLI calls. Saved work also expires {lifetime} seconds after its last read and may be evicted; an edited cursor names none."
        ),
        Refusal::OtherTool => {
            "This cursor was issued to another tool; send it to the tool its emitted call names."
                .into()
        }
        Refusal::OtherSurface => format!(
            "This cursor was {}; a cursor resumes only on the surface that issued it.",
            if scope.mcp {
                "issued by the CLI"
            } else {
                "issued by an MCP server"
            }
        ),
        Refusal::OtherAccess => {
            "This cursor was issued under another request credential; saved work answers only the credential it was issued to.".into()
        }
        Refusal::IdentityChanged => {
            "The current GitHub identity is not the one this cursor was issued to (it changed or could not be confirmed); saved work answers only the identity it was issued to.".into()
        }
        Refusal::RequestChanged => {
            "The saved request no longer validates to the rows its page was cut from under this runtime's contract.".into()
        }
    };
    RuntimeError::new(
        ErrorCode::ResponseSnapshotExpired,
        format!(
            "{cause} No source or provider was called. Rerun the original request without cursor."
        ),
    )
}

/// A saved page whose response was no longer retained, re-executed, and
/// whose sources now produce different content: the page it named no longer
/// exists, so the walk restarts.
pub(super) fn source_changed() -> RuntimeError {
    RuntimeError::new(
        ErrorCode::StaleSnapshot,
        "The saved response was no longer retained, so its rows were re-executed, and their sources now produce different content than the earlier pages. Rerun the original request without cursor.",
    )
}

/// Identity of an executed batch: validated rows in canonical key order,
/// original input indices, surface and request credential. The saved handle
/// carries this digest; an independent access digest verifies each replay.
pub(super) fn page_replay_key(
    tool: &str,
    mcp: bool,
    queries: &[CheckedQuery],
    indices: &[usize],
    credential: Option<&octocode_github::ResolvedCredential>,
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    // A request-supplied token partitions replays: one caller's page never
    // answers another caller's continuation.
    let credential = credential
        .map(|credential| octocode_engine::digest::sha256(credential.expose_secret().as_bytes()));
    let identity = json!({
        "tool": tool,
        "mcp": mcp,
        "queries": queries,
        "indices": indices,
        "credential": credential,
    });
    Sha256::digest(
        octocode_engine::digest::canonical_json(identity, octocode_engine::digest::Nulls::Drop)
            .to_string()
            .as_bytes(),
    )
    .into()
}
