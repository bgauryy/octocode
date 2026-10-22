//! Saved response pages: the settled envelope of a paged response, kept so
//! a later page replays it without re-executing the batch, and the saved
//! read that opens one.

use super::{RuntimeError, ToolRuntime};
use crate::config::{self, ConfigOutput};
use crate::contracts;
use crate::response::pager::{ResponsePageOptions, SavedPages};
use crate::response::result::FailureKind;
use octocode_contract_types::ErrorCode;

use base64::Engine as _;
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
    pub(super) structured: Value,
    pub(super) remaining: Vec<Vec<crate::response::pages::Remaining>>,
    pub(super) response_query: Value,
    pub(super) failure: Option<FailureKind>,
    pub(super) expires_at: String,
    pub(super) page_length: usize,
    pub(super) page_scope: String,
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

    fn key(&self, key: &[u8; 32], snapshot: &str) -> crate::cache::CacheKey {
        crate::cache::CacheKey {
            namespace: "response-pages-v1".into(),
            scope: format!("{}:{}", self.policy, hex::encode(key)),
            resource: snapshot.into(),
        }
    }

    fn get(&self, key: &[u8; 32], snapshot: &str) -> Option<Arc<PageReplay>> {
        self.store
            .get(&self.key(key, snapshot))
            .map(|hit| hit.value)
    }

    pub(super) fn insert(
        &self,
        key: &[u8; 32],
        snapshot: &str,
        replay: PageReplay,
    ) -> (Arc<PageReplay>, bool) {
        // The page memo a first read builds holds about one more copy of
        // the envelope (its row plan, bytes or text).
        let weight = octocode_engine::json_size::bytes(&replay)
            .map_or(usize::MAX, |bytes| bytes.saturating_mul(2));
        self.store.put_confirmed(
            self.key(key, snapshot),
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

fn decode_saved_snapshot(snapshot: &str) -> Option<([u8; 32], String)> {
    let (key, raw) = snapshot.strip_prefix("saved-v1:")?.split_once(':')?;
    let (kind, digest) = raw.rsplit_once(':')?;
    let base64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let key: [u8; 32] = base64.decode(key).ok()?.try_into().ok()?;
    let digest: [u8; 32] = base64.decode(digest).ok()?.try_into().ok()?;
    Some((key, format!("{kind}:{}", hex::encode(digest))))
}

pub(super) fn snapshot_expired() -> RuntimeError {
    RuntimeError::new(
        ErrorCode::ResponseSnapshotExpired,
        "The saved response is missing, expired or belongs to another tool, session or configuration. No source or provider was called. Pages expire after 120 seconds or earlier eviction; memory storage lasts only in the current runtime. Use persistent storage across CLI calls, or explicitly rerun without responseSnapshot and responseOffset.",
    )
}

/// Identity of an executed batch: validated rows in canonical key order,
/// original input indices, surface and request credential. The saved handle
/// carries this digest; an independent access digest verifies each replay.
pub(super) fn page_replay_key(
    tool: &str,
    mcp: bool,
    queries: &[Value],
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
        octocode_engine::digest::canonicalize_dropping_nulls(identity)
            .to_string()
            .as_bytes(),
    )
    .into()
}

impl ToolRuntime {
    /// The saved response a page read names, with `options` set to its
    /// stored page length and scope. A read carries no rows and keeps the
    /// saved length and scope; anything else, or a snapshot this caller
    /// cannot replay, fails without calling a source or provider.
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
                "Saved response reads accept only responseSnapshot, responseOffset, responseLength and responseScope; remove queries or start a new call without responseSnapshot.",
            ));
        }
        contracts::validate(tool, input.clone())
            .map_err(|error| super::admission::invalid_input_error(tool, input, error))?;
        let snapshot = options
            .response_snapshot
            .as_deref()
            .ok_or_else(snapshot_expired)?;
        let (key, raw) = decode_saved_snapshot(snapshot).ok_or_else(snapshot_expired)?;
        let replay = self
            .page_replays
            .get(&key, snapshot)
            .filter(|replay| replay.access == access)
            .ok_or_else(snapshot_expired)?;
        if options
            .response_length
            .is_some_and(|length| length != replay.page_length)
            || options
                .response_scope
                .as_deref()
                .is_some_and(|scope| scope != replay.page_scope)
        {
            return Err(RuntimeError::new(
                ErrorCode::ResponsePageMismatch,
                "Keep responseLength and responseScope unchanged when reading a saved response. Copy responsePagination.next; no source or provider was called.",
            ));
        }
        options.response_snapshot = Some(raw);
        options.response_length = Some(replay.page_length);
        options.response_scope = Some(replay.page_scope.clone());
        Ok((key, replay))
    }
}
