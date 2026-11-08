//! Saved response pages: the settled envelope of a paged response, kept so
//! a later page replays it without re-executing the batch, and the saved
//! read that opens one.

use super::RuntimeError;
use crate::build_identity::BUILD_IDENTITY;
use crate::config::{self, ConfigOutput};
use crate::response::cursor::Cursor;
use crate::response::pager::SavedPages;
use crate::response::result::FailureKind;
use crate::security::{CheckedQuery, Sanitized};
use octocode_contract_types::ErrorCode;

use crate::cache::{CONTINUATION_IDLE, CacheClass};
use crate::id::ToolId;
use crate::tools::clasify::sections::PLAN_IDLE;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

/// Immutable settled output. Page reads never consult its original sources.
#[derive(Serialize, Deserialize)]
pub(super) struct PageReplay {
    pub(super) access: [u8; 32],
    pub(super) github_identity: Option<String>,
    /// Sealed: a store reloaded from disk passes the output gate again.
    pub(super) structured: Sanitized,
    pub(super) response_query: Value,
    pub(super) failure: Option<FailureKind>,
    /// The text and page source later reads cut from (rebuilt from
    /// `structured` after a reload from disk).
    #[serde(skip)]
    pub(super) pages: SavedPages,
}

pub(super) struct PageReplayMemo {
    pub(super) store: crate::cache::Store<PageReplay>,
    /// Saved responses whose envelope is [`Sanitized::held`] (text of a
    /// credential-named source): kept in this process only, in either
    /// storage mode, as held provider answers are. A later runtime answers
    /// their cursors by re-executing the pinned page.
    held: crate::cache::Store<PageReplay>,
    pub(super) policy: String,
    pub(super) persistent: bool,
}

impl PageReplayMemo {
    pub(super) fn new(config: &ConfigOutput, cwd: &std::path::Path) -> Self {
        let persistent = config::saves_local_results(&config.resolved);
        let policy = replay_policy(config, cwd, BUILD_IDENTITY);
        Self {
            store: crate::cache::Store::new(
                crate::cache::CacheConfig::derived(),
                persistent.then(|| crate::cache::Directory::ResponsePages.path(&config.home)),
            ),
            held: crate::cache::Store::new(crate::cache::CacheConfig::derived(), None),
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
        let key = Self::key(cursor, &self.policy);
        self.held
            .get(&key)
            .or_else(|| self.store.get(&key))
            .map(|hit| hit.value)
    }

    pub(super) fn insert(
        &self,
        tool: &str,
        cursor: &Cursor,
        replay: PageReplay,
        deadline: std::time::Instant,
    ) -> (Arc<PageReplay>, bool) {
        // The page memo a first read builds holds about one more copy of
        // the envelope (its row plan, bytes or text).
        let weight = octocode_engine::json_size::bytes(&replay)
            .map_or(usize::MAX, |bytes| bytes.saturating_mul(2));
        let store = if replay.structured.held() {
            &self.held
        } else {
            &self.store
        };
        store.put_until(
            deadline,
            Self::key(cursor, &self.policy),
            replay,
            weight,
            retention_of(tool),
        )
    }
}

fn replay_policy(config: &ConfigOutput, cwd: &std::path::Path, build: &str) -> String {
    octocode_engine::digest::sha256(
        format!(
            "{}:{}:{}:{build}",
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

/// How long saved work for `tool` stays reachable after its last read: the
/// one owner of the lifetime of its cursors, saved pages, the expiry a page
/// states and the lifetime a refusal names. A clasify cursor names a plan
/// kept [`PLAN_IDLE`] after its last use, so its pages and cursor live that
/// long and never die before the plan they continue.
fn idle_lifetime(tool: &str) -> std::time::Duration {
    if tool == ToolId::Clasify.as_str() {
        PLAN_IDLE
    } else {
        CONTINUATION_IDLE
    }
}

/// The class saved work for `tool` is stored under: always idle, so every
/// read restarts the lifetime and [`page_expiry`] states the truth.
pub(super) fn retention_of(tool: &str) -> CacheClass {
    CacheClass::Idle {
        ttl: idle_lifetime(tool),
    }
}

/// The expiry a page of `tool` states: its read restarted the lifetime.
pub(super) fn page_expiry(tool: &str) -> String {
    crate::civil_date::iso8601_millis_after(idle_lifetime(tool))
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
    /// The current GitHub identity is not (or cannot be confirmed as) the
    /// one the handle was issued to.
    IdentityChanged,
    /// The saved request no longer admits to the rows its page was cut from.
    RequestChanged,
}

/// Where a refused cursor was sent: the storage and surface its text names,
/// and the tool whose saved-work lifetime it states.
#[derive(Clone, Copy)]
pub(super) struct RefusalScope {
    pub(super) persistent: bool,
    pub(super) tool: ToolId,
}

/// The error for `refusal`: its cause, then the recovery every refusal
/// shares. A handle that was never issued, or sent to the wrong tool, is the
/// caller's input (`invalidInput`); any other cause is saved work this
/// runtime cannot answer (`responseSnapshotExpired`).
pub(super) fn snapshot_expired(refusal: Refusal, scope: RefusalScope) -> RuntimeError {
    let lifetime = idle_lifetime(scope.tool.as_str()).as_secs();
    let cause = match refusal {
        Refusal::Malformed => {
            "Malformed cursor: it is not an issued handle (edited, truncated or invented).".into()
        }
        Refusal::NotRetained if scope.persistent => format!(
            "Unknown or expired cursor: no saved work under it for this tool, configuration, workspace and Octocode build. Saved work expires {lifetime} seconds after its last read and may be evicted; an edited cursor, or one issued under another configuration, workspace or Octocode build (an upgrade or rebuild), names none here."
        ),
        Refusal::NotRetained => format!(
            "Unknown or expired cursor: no saved work under it in this runtime. storage.mode is memory, or security.credentialFiles is allow, so saved work ends with the runtime that issued it (each CLI call is its own runtime); resuming across CLI calls needs storage.mode persistent with credential files withheld. Saved work also expires {lifetime} seconds after its last read and may be evicted; an edited cursor names none."
        ),
        Refusal::OtherTool => {
            "This cursor was issued to another tool; send it to the tool its emitted call names."
                .into()
        }
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
    let code = match refusal {
        Refusal::Malformed | Refusal::OtherTool => ErrorCode::InvalidInput,
        Refusal::NotRetained
        | Refusal::OtherAccess
        | Refusal::IdentityChanged
        | Refusal::RequestChanged => ErrorCode::ResponseSnapshotExpired,
    };
    RuntimeError::new(
        code,
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
/// original input indices and request credential. The surface (CLI or MCP) is
/// no part of it: one saved page resumes on either. The saved handle
/// carries this digest; an independent access digest verifies each replay.
pub(super) fn page_replay_key(
    tool: &str,
    queries: &[CheckedQuery],
    indices: &[usize],
    credential: Option<&octocode_github::ResolvedCredential>,
) -> [u8; 32] {
    // A request-supplied token partitions replays: one caller's page never
    // answers another caller's continuation.
    let credential = credential
        .map(|credential| octocode_engine::digest::sha256(credential.expose_secret().as_bytes()));
    let identity = json!({
        "tool": tool,
        "queries": queries,
        "indices": indices,
        "credential": credential,
    });
    octocode_engine::digest::sha256_digest(
        octocode_engine::digest::canonical_json(identity, octocode_engine::digest::Nulls::Drop)
            .to_string()
            .as_bytes(),
    )
}
