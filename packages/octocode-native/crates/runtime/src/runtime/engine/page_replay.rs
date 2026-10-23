//! Saved response pages: the settled envelope of a paged response, kept so
//! a later page replays it without re-executing the batch, and the saved
//! read that opens one.

use super::{RuntimeError, ToolRuntime};
use crate::config::{self, ConfigOutput};
use crate::contracts;
use crate::response::cursor::Cursor;
use crate::response::pager::{ResponsePageOptions, SavedPages};
use crate::response::result::FailureKind;
use crate::security::{CheckedQuery, Sanitized};
use octocode_contract_types::ErrorCode;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

const PAGE_REPLAY_TTL: Duration = Duration::from_secs(120);

/// Immutable settled output. Page reads never consult its original sources.
#[derive(Serialize, Deserialize)]
pub(super) struct PageReplay {
    pub(super) access: [u8; 32],
    pub(super) github_identity: Option<String>,
    /// Sealed: a store reloaded from disk passes the output gate again.
    pub(super) structured: Sanitized,
    pub(super) remaining: Vec<Vec<crate::response::pages::Remaining>>,
    pub(super) response_query: Value,
    pub(super) failure: Option<FailureKind>,
    pub(super) expires_at: String,
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
        crate::cache::CacheConfig {
            max_entries: 8,
            max_bytes: 32 * 1024 * 1024,
            disk_bytes: 128 * 1024 * 1024,
        }
    }

    pub(super) fn new(config: &ConfigOutput, cwd: &std::path::Path) -> Self {
        let persistent = config::is_persistent_storage_enabled(&config.resolved);
        let policy = octocode_engine::digest::sha256(
            format!(
                "{}:{}:{}",
                config.identity_digest(),
                cwd.display(),
                octocode_contract_types::TOOL_TYPES_CONTRACT_FINGERPRINT
            )
            .as_bytes(),
        );
        Self {
            store: crate::cache::Store::new(
                Self::cache_config(),
                persistent.then(|| config.home.join("tmp").join("response-pages")),
            ),
            policy,
            persistent,
        }
    }

    pub(super) fn storage(&self) -> &'static str {
        if self.persistent {
            "persistent"
        } else {
            "memory"
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

    fn get(&self, cursor: &Cursor) -> Option<Arc<PageReplay>> {
        self.store
            .get(&Self::key(cursor, &self.policy))
            .map(|hit| hit.value)
    }

    pub(super) fn insert(&self, cursor: &Cursor, replay: PageReplay) -> (Arc<PageReplay>, bool) {
        // The page memo a first read builds holds about one more copy of
        // the envelope (its row plan, bytes or text).
        let weight = octocode_engine::json_size::bytes(&replay)
            .map_or(usize::MAX, |bytes| bytes.saturating_mul(2));
        self.store.put_confirmed(
            Self::key(cursor, &self.policy),
            replay,
            weight,
            crate::cache::CacheClass::Volatile {
                ttl: PAGE_REPLAY_TTL,
            },
        )
    }
}

pub(super) fn page_expiry() -> String {
    crate::civil_date::iso8601_millis_after(PAGE_REPLAY_TTL)
}

pub(super) fn snapshot_expired() -> RuntimeError {
    RuntimeError::new(
        ErrorCode::ResponseSnapshotExpired,
        "The saved response is missing, expired or belongs to another tool, session or configuration. No source or provider was called. Pages expire after 120 seconds or earlier eviction; memory storage lasts only in the current runtime. Use persistent storage across CLI calls, or explicitly rerun without responseSnapshot.",
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

impl ToolRuntime {
    /// The saved response a page read's cursor names, with `options` set to
    /// the page it asks for: content snapshot, scope, length and offset. A
    /// read carries no rows, and a `responseLength` or `responseScope` sent
    /// beside the cursor must match it; anything else, or a cursor this
    /// caller cannot replay, fails without calling a source or provider.
    pub(super) fn open_saved(
        &self,
        tool: &str,
        input: &Value,
        options: &mut ResponsePageOptions,
        access: [u8; 32],
    ) -> Result<([u8; 32], Arc<PageReplay>), RuntimeError> {
        if input.get("queries").is_some() {
            return Err(RuntimeError::new(
                ErrorCode::InvalidInput,
                "Saved response reads take only responseSnapshot; remove queries or start a new call without responseSnapshot.",
            ));
        }
        contracts::validate(tool, input.clone())
            .map_err(|error| super::admission::invalid_input_error(tool, input, error))?;
        let cursor = options
            .response_snapshot
            .as_deref()
            .and_then(Cursor::decode)
            .ok_or_else(snapshot_expired)?;
        let replay = self
            .page_replays
            .get(&cursor)
            .filter(|replay| replay.access == access)
            .ok_or_else(snapshot_expired)?;
        if options
            .response_length
            .is_some_and(|length| length != cursor.length)
            || options
                .response_scope
                .is_some_and(|scope| scope != cursor.scope)
        {
            return Err(RuntimeError::new(
                ErrorCode::ResponsePageMismatch,
                "Keep responseLength and responseScope unchanged when reading a saved response. Copy responsePagination.next; no source or provider was called.",
            ));
        }
        options.open(&cursor);
        Ok((cursor.key, replay))
    }
}
