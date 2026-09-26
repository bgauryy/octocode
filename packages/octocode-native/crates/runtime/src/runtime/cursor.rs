use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Deserialize bytes, verify scope / contract / expiry.
fn deserialize_and_check<T: DeserializeOwned + CursorFields>(
    bytes: &[u8],
    scope: &str,
) -> Result<T, CursorError> {
    let cursor: T = serde_json::from_slice(bytes).map_err(|_| CursorError::Invalid)?;
    if cursor.version() != 1 {
        return Err(CursorError::Invalid);
    }
    if cursor.contract() != crate::contracts::contract_fingerprint() {
        return Err(CursorError::StaleContract);
    }
    if cursor.scope() != scope {
        return Err(CursorError::ChangedScope);
    }
    if cursor.expires_at() < now()? {
        return Err(CursorError::Expired);
    }
    Ok(cursor)
}

/// Accessor trait so `deserialize_and_check` can read the common header fields.
trait CursorFields {
    fn version(&self) -> u32;
    fn contract(&self) -> &str;
    fn scope(&self) -> &str;
    fn expires_at(&self) -> u64;
}

// ── Universal cursor (all tools, no file-system source SHA) ───────────────
//
// Encodes any (tool, query) pair. Scope-locked, contract-locked, 24 h TTL.
// Suitable for GitHub, LSP, AST, artifact, and any remote tool.

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UniversalCursor {
    version: u32,
    contract: String,
    scope: String,
    expires_at: u64,
    pub tool: String,
    pub query: Value,
}

impl CursorFields for UniversalCursor {
    fn version(&self) -> u32 {
        self.version
    }
    fn contract(&self) -> &str {
        &self.contract
    }
    fn scope(&self) -> &str {
        &self.scope
    }
    fn expires_at(&self) -> u64 {
        self.expires_at
    }
}

impl UniversalCursor {
    /// Test-only: responses carry replayable `query` continuations, not tokens.
    #[cfg(test)]
    pub fn create(tool: &str, query: Value, scope: String) -> Result<String, CursorError> {
        encode_to_token(&Self {
            version: 1,
            contract: crate::contracts::contract_fingerprint().into(),
            scope,
            expires_at: now()? + TOKEN_LIFETIME.as_secs(),
            tool: tool.into(),
            query,
        })
    }

    pub fn decode(token: &str, scope: &str) -> Result<Self, CursorError> {
        let bytes = decode_raw(token)?;
        let cursor: Self = deserialize_and_check(&bytes, scope)?;
        if cursor.tool.is_empty() || !cursor.query.is_object() {
            return Err(CursorError::Invalid);
        }
        Ok(cursor)
    }
}

// ── File-backed read cursor (localFetch / localSearch) ────────────────────
//
// Adds `source_sha256` for file-integrity verification at resume time.

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadCursor {
    version: u32,
    contract: String,
    scope: String,
    expires_at: u64,
    pub tool: String,
    pub query: Value,
    pub source_sha256: String,
}

impl CursorFields for ReadCursor {
    fn version(&self) -> u32 {
        self.version
    }
    fn contract(&self) -> &str {
        &self.contract
    }
    fn scope(&self) -> &str {
        &self.scope
    }
    fn expires_at(&self) -> u64 {
        self.expires_at
    }
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
    /// Test-only: localFetch continuations carry `snapshot`, not tokens.
    #[cfg(test)]
    pub fn create(
        tool: &str,
        query: Value,
        source_sha256: String,
        scope: String,
    ) -> Result<String, CursorError> {
        if !matches!(tool, "localFetch" | "localSearch")
            || !query.is_object()
            || source_sha256.is_empty()
        {
            return Err(CursorError::Invalid);
        }
        if tool == "localSearch" && query["snapshot"].as_str() != Some(&source_sha256) {
            return Err(CursorError::Invalid);
        }
        encode_to_token(&Self {
            version: 1,
            contract: crate::contracts::contract_fingerprint().into(),
            scope,
            expires_at: now()? + TOKEN_LIFETIME.as_secs(),
            tool: tool.into(),
            query,
            source_sha256,
        })
    }

    pub fn decode(token: &str, scope: &str) -> Result<Self, CursorError> {
        let bytes = decode_raw(token)?;
        let cursor: Self = deserialize_and_check(&bytes, scope)?;
        if !matches!(cursor.tool.as_str(), "localFetch" | "localSearch")
            || !cursor.query.is_object()
            || cursor.source_sha256.is_empty()
        {
            return Err(CursorError::Invalid);
        }
        if cursor.tool == "localSearch"
            && cursor.query["snapshot"].as_str() != Some(&cursor.source_sha256)
        {
            return Err(CursorError::Invalid);
        }
        Ok(cursor)
    }

    pub fn verify_source(
        &self,
        paths: &crate::policy::path::PathPolicy,
    ) -> Result<(), CursorError> {
        if self.tool == "localSearch" {
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
        let token =
            UniversalCursor::create("astSearch", query.clone(), "scope".into()).expect("cursor");
        let cursor = UniversalCursor::decode(&token, "scope").expect("decode");
        assert_eq!(cursor.tool, "astSearch");
        assert_eq!(cursor.query, query);
        assert!(matches!(
            UniversalCursor::decode(&token, "other-scope"),
            Err(CursorError::ChangedScope)
        ));
    }

    #[test]
    fn read_tokens_round_trip_with_their_row_source_digest() {
        let query = serde_json::json!({"path":"/workspace/a.rs","offset":10});
        let token = ReadCursor::create(
            "localFetch",
            query.clone(),
            "digest-a".into(),
            "scope".into(),
        )
        .expect("read cursor");
        let cursor = ReadCursor::decode(&token, "scope").expect("decode read cursor");
        assert_eq!(cursor.tool, "localFetch");
        assert_eq!(cursor.query, query);
        assert_eq!(cursor.source_sha256, "digest-a");
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
            ReadCursor::decode("not:a:token", "scope"),
            Err(CursorError::Invalid)
        ));
        assert!(matches!(
            ReadCursor::decode(&"x".repeat(200_000), "scope"),
            Err(CursorError::Invalid)
        ));
    }
}
