use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_TOKEN_BYTES: usize = 64 * 1024;
const TOKEN_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);

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

fn source_digest(path: &Path) -> Result<String, CursorError> {
    let mut file = File::open(path).map_err(|_| CursorError::SourceUnavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_| CursorError::SourceUnavailable)?;
    if !metadata.is_file() {
        return Err(CursorError::SourceUnavailable);
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        if Instant::now() >= deadline {
            return Err(CursorError::Timeout);
        }
        let length = file
            .read(&mut buffer)
            .map_err(|_| CursorError::SourceUnavailable)?;
        if length == 0 {
            break;
        }
        hash.update(&buffer[..length]);
    }
    let after = file
        .metadata()
        .map_err(|_| CursorError::SourceUnavailable)?;
    if metadata.len() != after.len() || metadata.modified().ok() != after.modified().ok() {
        return Err(CursorError::ChangedSource);
    }
    Ok(hex::encode(hash.finalize()))
}

fn now() -> Result<u64, CursorError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| CursorError::Invalid)
}

impl ReadCursor {
    /// Search execution revalidates this snapshot after reapplying path policy.
    pub fn create_search(query: Value, scope: String) -> Result<String, CursorError> {
        let snapshot = query["snapshot"]
            .as_str()
            .filter(|s| s.starts_with("lexical-live-v1:"))
            .ok_or(CursorError::Invalid)?
            .to_owned();
        Self {
            version: 1,
            contract: crate::contracts::contract_fingerprint().into(),
            scope,
            expires_at: now()? + TOKEN_LIFETIME.as_secs(),
            tool: "localSearch".into(),
            query,
            source_sha256: snapshot,
        }
        .encode()
    }
    /// `path` must have passed the current path policy; tokens grant no authority.
    pub fn create(
        query: Value,
        scope: String,
        validated_path: &Path,
        source_sha256: Option<&str>,
    ) -> Result<String, CursorError> {
        let current_digest = source_digest(validated_path)?;
        if source_sha256.is_some_and(|expected| current_digest != expected) {
            return Err(CursorError::ChangedSource);
        }
        let cursor = Self {
            version: 1,
            contract: crate::contracts::contract_fingerprint().into(),
            scope,
            expires_at: now()? + TOKEN_LIFETIME.as_secs(),
            tool: "localFetch".into(),
            query,
            source_sha256: current_digest,
        };
        cursor.encode()
    }

    fn encode(&self) -> Result<String, CursorError> {
        let bytes = serde_json::to_vec(self).map_err(|_| CursorError::Invalid)?;
        if bytes.len() > MAX_TOKEN_BYTES {
            return Err(CursorError::Invalid);
        }
        Ok(format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(&bytes),
            hex::encode(Sha256::digest(&bytes))
        ))
    }

    pub fn decode(token: &str, scope: &str) -> Result<Self, CursorError> {
        if token.len() > MAX_TOKEN_BYTES * 2 {
            return Err(CursorError::Invalid);
        }
        let (encoded, checksum) = token.split_once('.').ok_or(CursorError::Invalid)?;
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| CursorError::Invalid)?;
        if checksum != hex::encode(Sha256::digest(&bytes)) {
            return Err(CursorError::Invalid);
        }
        let cursor: Self = serde_json::from_slice(&bytes).map_err(|_| CursorError::Invalid)?;
        if cursor.version != 1
            || !matches!(cursor.tool.as_str(), "localFetch" | "localSearch")
            || !cursor.query.is_object()
        {
            return Err(CursorError::Invalid);
        }
        if cursor.tool == "localSearch"
            && cursor.query["snapshot"].as_str() != Some(&cursor.source_sha256)
        {
            return Err(CursorError::Invalid);
        }
        if cursor.contract != crate::contracts::contract_fingerprint() {
            return Err(CursorError::StaleContract);
        }
        if cursor.scope != scope {
            return Err(CursorError::ChangedScope);
        }
        if cursor.expires_at < now()? {
            return Err(CursorError::Expired);
        }
        Ok(cursor)
    }

    /// Call after reapplying current authorization to the cursor query path.
    pub fn verify_source(&self, validated_path: &Path) -> Result<(), CursorError> {
        if source_digest(validated_path)? != self.source_sha256 {
            return Err(CursorError::ChangedSource);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_and_foreign_tokens_are_rejected_before_file_access() {
        assert!(matches!(
            ReadCursor::decode("not:a:token", "scope"),
            Err(CursorError::Invalid)
        ));
        let cursor = ReadCursor {
            version: 1,
            contract: crate::contracts::contract_fingerprint().into(),
            scope: "one".into(),
            expires_at: now().expect("clock") + 60,
            tool: "localFetch".into(),
            query: serde_json::json!({"path":"/never/read"}),
            source_sha256: "unused".into(),
        };
        let bytes = serde_json::to_vec(&cursor).expect("serialize");
        let token = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(&bytes),
            hex::encode(Sha256::digest(&bytes))
        );
        assert!(matches!(
            ReadCursor::decode(&token, "two"),
            Err(CursorError::ChangedScope)
        ));
    }
}
