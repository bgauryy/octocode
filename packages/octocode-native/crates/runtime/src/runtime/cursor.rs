use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::tools::id::ToolId;

const MAX_TOKEN_BYTES: usize = 64 * 1024;
#[cfg(test)]
const TOKEN_LIFETIME: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

type HmacSha256 = Hmac<Sha256>;

/// Per-process, random cursor-signing key. Tokens are authenticated with
/// HMAC-SHA256 under this key so a caller cannot forge a cursor payload (a bare
/// SHA-256 checksum only detects accidental corruption — any caller can recompute
/// it). The key never leaves the process and is regenerated each start, so a
/// token minted by one process is not honored by another; that is intentional —
/// pagination cursors are session-scoped and a client simply re-runs the query.
fn cursor_signing_key() -> &'static [u8; 32] {
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    KEY.get_or_init(|| {
        let mut key = [0_u8; 32];
        if getrandom::fill(&mut key).is_ok() {
            return key;
        }
        // OS CSPRNG unavailable (extremely rare). Derive a process-local,
        // non-persistent fallback from std's OS-seeded RandomState so signing
        // still functions without panicking. Tokens remain unforgeable across
        // processes; within a process the key is stable.
        use std::hash::{BuildHasher, Hasher};
        let state = std::collections::hash_map::RandomState::new();
        for (index, slot) in key.iter_mut().enumerate() {
            let mut hasher = state.build_hasher();
            hasher.write_usize(index);
            hasher.write_u32(std::process::id());
            *slot = (hasher.finish() & 0xff) as u8;
        }
        key
    })
}

/// HMAC-SHA256 tag (hex) of `bytes` under the per-process signing key.
#[cfg(test)]
fn sign_payload(bytes: &[u8]) -> Result<String, CursorError> {
    let mut mac =
        HmacSha256::new_from_slice(cursor_signing_key()).map_err(|_| CursorError::Invalid)?;
    mac.update(bytes);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

/// Constant-time verification of a hex-encoded HMAC tag against `bytes`.
fn verify_payload(bytes: &[u8], tag_hex: &str) -> bool {
    let Ok(tag) = hex::decode(tag_hex) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(cursor_signing_key()) else {
        return false;
    };
    mac.update(bytes);
    mac.verify_slice(&tag).is_ok()
}

// ── Opaque scoped state tokens (artifact pagination) ──────────────────────
//
// Artifact cursors travel INSIDE the continuation query, and the CLI prints
// that query as a command to re-run in a fresh process. They are therefore
// signed with a persistent per-user key (`<octocode home>/cursor.key`, 0600)
// rather than the per-process key, so a printed continuation replays across
// invocations while a caller-constructed cursor is still rejected.

/// Prefix marking a signed opaque-state token (legacy states are raw JSON).
pub const SIGNED_STATE_PREFIX: &str = "s1.";

fn load_or_create_key(home: &Path) -> Option<[u8; 32]> {
    let path = home.join("cursor.key");
    if let Ok(bytes) = std::fs::read(&path)
        && bytes.len() == 32
    {
        let mut key = [0_u8; 32];
        key.copy_from_slice(&bytes);
        return Some(key);
    }
    let mut key = [0_u8; 32];
    getrandom::fill(&mut key).ok()?;
    let _ = std::fs::create_dir_all(home);
    crate::cache::write_private(&path, &key).ok()?;
    Some(key)
}

/// Per-user signing key, resolved once per process. Falls back to the
/// per-process key when the home is unavailable or unwritable — signing
/// still works, tokens then just verify only within this process.
pub fn user_signing_key(home: Option<&Path>) -> &'static [u8; 32] {
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    KEY.get_or_init(|| {
        home.and_then(load_or_create_key)
            .unwrap_or(*cursor_signing_key())
    })
}

/// Sign opaque provider state bound to `scope`:
/// `s1.<base64url(payload)>.<hmac-sha256 hex>`.
pub fn sign_state(key: &[u8; 32], scope: &str, payload: &[u8]) -> Result<String, CursorError> {
    if payload.len() > MAX_TOKEN_BYTES {
        return Err(CursorError::Invalid);
    }
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| CursorError::Invalid)?;
    mac.update(scope.as_bytes());
    mac.update(&[0]);
    mac.update(payload);
    let tag = hex::encode(mac.finalize().into_bytes());
    Ok(format!(
        "{SIGNED_STATE_PREFIX}{}.{tag}",
        URL_SAFE_NO_PAD.encode(payload)
    ))
}

/// Constant-time verification of a signed opaque-state token bound to
/// `scope`; returns the payload bytes.
pub fn verify_state(key: &[u8; 32], scope: &str, token: &str) -> Result<Vec<u8>, CursorError> {
    if token.len() > MAX_TOKEN_BYTES * 2 {
        return Err(CursorError::Invalid);
    }
    let rest = token
        .strip_prefix(SIGNED_STATE_PREFIX)
        .ok_or(CursorError::Invalid)?;
    let (encoded, tag_hex) = rest.split_once('.').ok_or(CursorError::Invalid)?;
    let payload = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| CursorError::Invalid)?;
    let tag = hex::decode(tag_hex).map_err(|_| CursorError::Invalid)?;
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| CursorError::Invalid)?;
    mac.update(scope.as_bytes());
    mac.update(&[0]);
    mac.update(&payload);
    mac.verify_slice(&tag).map_err(|_| CursorError::Invalid)?;
    Ok(payload)
}

// ── Shared encode / decode primitives ─────────────────────────────────────

/// Serialize `cursor` to a base64url payload with an authenticated HMAC suffix.
#[cfg(test)]
fn encode_to_token<T: Serialize>(cursor: &T) -> Result<String, CursorError> {
    let bytes = serde_json::to_vec(cursor).map_err(|_| CursorError::Invalid)?;
    if bytes.len() > MAX_TOKEN_BYTES {
        return Err(CursorError::Invalid);
    }
    let tag = sign_payload(&bytes)?;
    Ok(format!("{}.{}", URL_SAFE_NO_PAD.encode(&bytes), tag))
}

/// Verify the HMAC tag and return the raw JSON bytes; does NOT deserialize.
fn decode_raw(token: &str) -> Result<Vec<u8>, CursorError> {
    if token.len() > MAX_TOKEN_BYTES * 2 {
        return Err(CursorError::Invalid);
    }
    let (encoded, checksum) = token.split_once('.').ok_or(CursorError::Invalid)?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| CursorError::Invalid)?;
    if !verify_payload(&bytes, checksum) {
        return Err(CursorError::Invalid);
    }
    Ok(bytes)
}

// ── Resume cursor: one token, one kind discriminator ─────────────────────
//
// Every token carries the same header (scope-, contract- and TTL-locked) and a
// `kind`. A universal token encodes any (tool, query) pair; a read token
// (localFetch / localSearch) also pins the source digest checked at resume.
// A token is decoded once, so each kind reports its own failure.

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum CursorKind {
    Universal,
    Read,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CursorPayload {
    version: u32,
    contract: String,
    scope: String,
    expires_at: u64,
    kind: CursorKind,
    tool: String,
    query: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_sha256: Option<String>,
}

/// A decoded `{cursor}` token.
pub enum Cursor {
    /// Any tool: GitHub, LSP, AST, artifact, and other remote tools.
    Universal { tool: String, query: Value },
    /// A file-backed read that must still match its source at resume.
    Read(ReadCursor),
}

/// File-backed read cursor (localFetch / localSearch).
pub struct ReadCursor {
    pub tool: String,
    pub query: Value,
    pub source_sha256: String,
}

impl Cursor {
    /// Test-only: responses carry replayable `query` continuations, not tokens.
    #[cfg(test)]
    pub fn universal(tool: &str, query: Value, scope: String) -> Result<String, CursorError> {
        encode(scope, CursorKind::Universal, tool, query, None)
    }

    /// Test-only: localFetch continuations carry `snapshot`, not tokens.
    #[cfg(test)]
    pub fn read(
        tool: &str,
        query: Value,
        source_sha256: String,
        scope: String,
    ) -> Result<String, CursorError> {
        let read = ReadCursor::checked(tool.into(), query, source_sha256)?;
        encode(
            scope,
            CursorKind::Read,
            &read.tool,
            read.query,
            Some(read.source_sha256),
        )
    }

    pub fn decode(token: &str, scope: &str) -> Result<Self, CursorError> {
        let bytes = decode_raw(token)?;
        let payload: CursorPayload =
            serde_json::from_slice(&bytes).map_err(|_| CursorError::Invalid)?;
        if payload.version != 1 {
            return Err(CursorError::Invalid);
        }
        if payload.contract != crate::contracts::contract_fingerprint() {
            return Err(CursorError::StaleContract);
        }
        if payload.scope != scope {
            return Err(CursorError::ChangedScope);
        }
        if payload.expires_at < now()? {
            return Err(CursorError::Expired);
        }
        if payload.tool.is_empty() || !payload.query.is_object() {
            return Err(CursorError::Invalid);
        }
        match (payload.kind, payload.source_sha256) {
            (CursorKind::Universal, None) => Ok(Self::Universal {
                tool: payload.tool,
                query: payload.query,
            }),
            (CursorKind::Read, Some(source_sha256)) => {
                ReadCursor::checked(payload.tool, payload.query, source_sha256).map(Self::Read)
            }
            _ => Err(CursorError::Invalid),
        }
    }

    /// Source-digest check; only read cursors carry one.
    pub fn verify_source(
        &self,
        paths: &crate::policy::path::PathPolicy,
    ) -> Result<(), CursorError> {
        match self {
            Self::Universal { .. } => Ok(()),
            Self::Read(read) => read.verify_source(paths),
        }
    }

    /// The resumed tool name and query.
    pub fn into_parts(self) -> (String, Value) {
        match self {
            Self::Universal { tool, query } | Self::Read(ReadCursor { tool, query, .. }) => {
                (tool, query)
            }
        }
    }
}

#[cfg(test)]
fn encode(
    scope: String,
    kind: CursorKind,
    tool: &str,
    query: Value,
    source_sha256: Option<String>,
) -> Result<String, CursorError> {
    if tool.is_empty() || !query.is_object() {
        return Err(CursorError::Invalid);
    }
    encode_to_token(&CursorPayload {
        version: 1,
        contract: crate::contracts::contract_fingerprint().into(),
        scope,
        expires_at: now()? + TOKEN_LIFETIME.as_secs(),
        kind,
        tool: tool.into(),
        query,
        source_sha256,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CursorError {
    Invalid,
    Expired,
    StaleContract,
    ChangedScope,
    ChangedSource,
    SourceUnavailable,
    Timeout,
}

/// Request fields that state intent or diagnostics, never which results a
/// page holds: a replayed continuation may carry them differently, so every
/// page digest ignores them.
pub const INTENT_FIELDS: [&str; 3] = ["mainGoal", "reasoning", "debug"];

pub fn scope_digest(value: &Value) -> Result<String, CursorError> {
    let bytes = serde_json::to_vec(value).map_err(|_| CursorError::Invalid)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn now() -> Result<u64, CursorError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| CursorError::Invalid)
}

impl ReadCursor {
    /// Only localFetch / localSearch read cursors exist, and a localSearch
    /// cursor's snapshot is its source digest.
    fn checked(tool: String, query: Value, source_sha256: String) -> Result<Self, CursorError> {
        if !matches!(
            ToolId::from_name(&tool),
            Some(ToolId::LocalFetch | ToolId::LocalSearch)
        ) || !query.is_object()
            || source_sha256.is_empty()
        {
            return Err(CursorError::Invalid);
        }
        if tool == ToolId::LocalSearch.as_str()
            && query["snapshot"].as_str() != Some(&source_sha256)
        {
            return Err(CursorError::Invalid);
        }
        Ok(Self {
            tool,
            query,
            source_sha256,
        })
    }

    pub fn verify_source(
        &self,
        paths: &crate::policy::path::PathPolicy,
    ) -> Result<(), CursorError> {
        if self.tool == ToolId::LocalSearch.as_str() {
            // localSearch validates its directory snapshot during execution.
            return Ok(());
        }
        let path = self
            .query
            .get("path")
            .and_then(Value::as_str)
            .ok_or(CursorError::Invalid)?;
        let validated = paths
            .validate_read(path)
            .map_err(|_| CursorError::SourceUnavailable)?;
        let mut file =
            std::fs::File::open(validated.canonical).map_err(|_| CursorError::SourceUnavailable)?;
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|_| CursorError::SourceUnavailable)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        if hex::encode(digest.finalize()) != self.source_sha256 {
            return Err(CursorError::ChangedSource);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn universal_tokens_round_trip_without_granting_execution_authority() {
        let query = serde_json::json!({"operation":"symbols","path":"/workspace"});
        let token = Cursor::universal("astSearch", query.clone(), "scope".into()).expect("cursor");
        let Ok(Cursor::Universal {
            tool,
            query: decoded,
        }) = Cursor::decode(&token, "scope")
        else {
            panic!("universal token decodes as the universal kind");
        };
        assert_eq!(tool, "astSearch");
        assert_eq!(decoded, query);
        assert!(matches!(
            Cursor::decode(&token, "other-scope"),
            Err(CursorError::ChangedScope)
        ));
    }

    #[test]
    fn read_tokens_round_trip_with_their_row_source_digest() {
        let query = serde_json::json!({"path":"/workspace/a.rs","offset":10});
        let token = Cursor::read(
            "localFetch",
            query.clone(),
            "digest-a".into(),
            "scope".into(),
        )
        .expect("read cursor");
        let Ok(Cursor::Read(cursor)) = Cursor::decode(&token, "scope") else {
            panic!("read token decodes as the read kind");
        };
        assert_eq!(cursor.tool, "localFetch");
        assert_eq!(cursor.query, query);
        assert_eq!(cursor.source_sha256, "digest-a");
    }

    #[test]
    fn a_kind_without_its_matching_digest_field_is_invalid() {
        let payload = |kind: CursorKind, source_sha256: Option<String>| {
            encode_to_token(&CursorPayload {
                version: 1,
                contract: crate::contracts::contract_fingerprint().into(),
                scope: "scope".into(),
                expires_at: now().expect("clock") + TOKEN_LIFETIME.as_secs(),
                kind,
                tool: "localFetch".into(),
                query: serde_json::json!({"path":"/workspace/a.rs"}),
                source_sha256,
            })
            .expect("token")
        };
        for token in [
            payload(CursorKind::Universal, Some("digest".into())),
            payload(CursorKind::Read, None),
        ] {
            assert!(matches!(
                Cursor::decode(&token, "scope"),
                Err(CursorError::Invalid)
            ));
        }
    }

    #[test]
    fn user_key_persists_across_loads_and_survives_reload() {
        let home = std::env::temp_dir().join(format!(
            "octocode-cursor-key-{}-{:?}",
            std::process::id(),
            std::time::Instant::now()
        ));
        std::fs::create_dir_all(&home).expect("test fixture operation should succeed");
        let first = load_or_create_key(&home).expect("create key");
        let second = load_or_create_key(&home).expect("reload key");
        assert_eq!(first, second, "reload must return the persisted key");
        assert!(home.join("cursor.key").is_file());
        let token = sign_state(&first, "scope", b"{\"page\":2}").expect("sign");
        assert_eq!(
            verify_state(&second, "scope", &token).expect("verify"),
            b"{\"page\":2}"
        );
        assert!(verify_state(&second, "other", &token).is_err());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn corrupt_and_foreign_tokens_are_rejected_before_file_access() {
        assert!(matches!(
            Cursor::decode("not:a:token", "scope"),
            Err(CursorError::Invalid)
        ));
        assert!(matches!(
            Cursor::decode(&"x".repeat(200_000), "scope"),
            Err(CursorError::Invalid)
        ));
    }
}
