//! Main-compatible encrypted credentials. File mutation is serialized and atomic.
use super::super::{ProviderError, ProviderErrorKind};
use super::{StoredCredentials, normalize_host};
use aes_gcm::{
    AesGcm,
    aead::{Aead, KeyInit, consts::U16},
    aes::Aes256,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

// Main uses a 16-byte IV, not the 12-byte Aes256Gcm alias.
type MainCipher = AesGcm<Aes256, U16>;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Document {
    version: u32,
    credentials: BTreeMap<String, StoredCredentials>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            version: 1,
            credentials: BTreeMap::new(),
        }
    }
}
#[derive(Clone)]
pub(super) struct HomeStore {
    home: PathBuf,
}
impl HomeStore {
    pub(super) fn new(home: &Path) -> Self {
        Self { home: home.into() }
    }
    pub(super) fn load(&self, host: &str) -> Result<Option<StoredCredentials>, ProviderError> {
        if !exists(&self.home.join("credentials.json"))? {
            return Ok(None);
        }
        let _lock = self.lock()?;
        Ok(self.read()?.credentials.remove(&normalize_host(host)))
    }
    pub(super) fn save(&self, credentials: &StoredCredentials) -> Result<(), ProviderError> {
        self.save_checked(credentials, None)
    }
    pub(super) fn replace(
        &self,
        previous: &StoredCredentials,
        credentials: &StoredCredentials,
    ) -> Result<(), ProviderError> {
        self.save_checked(credentials, Some(previous))
    }
    fn save_checked(
        &self,
        credentials: &StoredCredentials,
        previous: Option<&StoredCredentials>,
    ) -> Result<(), ProviderError> {
        let host = normalize_host(&credentials.hostname);
        if host.is_empty() || credentials.token.token.trim().is_empty() {
            return Err(failure("invalid stored credential"));
        }
        self.ensure_home()?;
        let _lock = self.lock()?;
        let mut document = self.read()?;
        if let Some(previous) = previous
            && document.credentials.get(&host) != Some(previous)
        {
            return Err(ProviderError::new(
                ProviderErrorKind::Authentication,
                "credential changed during refresh; retry authentication",
            ));
        }
        let mut credentials = credentials.clone();
        credentials.hostname = host.clone();
        document.credentials.insert(host, credentials);
        self.write(&document)
    }
    pub(super) fn delete(&self, host: &str) -> Result<(), ProviderError> {
        if !exists(&self.home.join("credentials.json"))? {
            return Ok(());
        }
        let _lock = self.lock()?;
        let mut document = self.read()?;
        if document.credentials.remove(&normalize_host(host)).is_none() {
            return Ok(());
        }
        if document.credentials.is_empty() {
            fs::remove_file(self.home.join("credentials.json"))
                .map_err(|_| failure("cannot remove credentials"))?;
            fs::remove_file(self.home.join(".key"))
                .map_err(|_| failure("cannot remove credential key"))?;
            self.sync_home()?;
            Ok(())
        } else {
            self.write(&document)
        }
    }
    fn read(&self) -> Result<Document, ProviderError> {
        let Some(encoded) = read_private(&self.home.join("credentials.json"), MAX_FILE_BYTES)?
        else {
            return Ok(Document::default());
        };
        let key = self
            .read_key()?
            .ok_or_else(|| failure("credential key is missing"))?;
        let cipher =
            MainCipher::new_from_slice(&key).map_err(|_| failure("invalid credential key"))?;
        let text =
            std::str::from_utf8(&encoded).map_err(|_| failure("invalid encrypted credentials"))?;
        let parts: Vec<_> = text.trim().split(':').collect();
        if parts.len() != 3 {
            return Err(failure("invalid encrypted credentials"));
        }
        let iv: [u8; 16] = decode(parts[0])?
            .try_into()
            .map_err(|_| failure("invalid credential IV"))?;
        let tag: [u8; 16] = decode(parts[1])?
            .try_into()
            .map_err(|_| failure("invalid credential tag"))?;
        let mut ciphertext = decode(parts[2])?;
        ciphertext.extend_from_slice(&tag);
        let plaintext = cipher
            .decrypt(&iv.into(), ciphertext.as_ref())
            .map_err(|_| failure("credential authentication failed"))?;
        let document: Document = serde_json::from_slice(&plaintext)
            .map_err(|_| failure("invalid credential document"))?;
        if document.version != 1
            || document.credentials.iter().any(|(host, value)| {
                normalize_host(&value.hostname) != *host || value.token.token.trim().is_empty()
            })
        {
            return Err(failure("unsupported or invalid credential document"));
        }
        Ok(document)
    }
    fn read_key(&self) -> Result<Option<[u8; 32]>, ProviderError> {
        read_private(&self.home.join(".key"), 128)?
            .map(|key| {
                let text =
                    std::str::from_utf8(&key).map_err(|_| failure("invalid credential key"))?;
                decode(text.trim())?
                    .try_into()
                    .map_err(|_| failure("invalid credential key"))
            })
            .transpose()
    }
    fn write(&self, document: &Document) -> Result<(), ProviderError> {
        let key = match self.read_key()? {
            Some(key) => key,
            None => {
                let mut key = [0u8; 32];
                random(&mut key)?;
                self.atomic_write(".key", hex::encode(key).as_bytes())?;
                key
            }
        };
        let cipher =
            MainCipher::new_from_slice(&key).map_err(|_| failure("invalid credential key"))?;
        let mut iv = [0u8; 16];
        random(&mut iv)?;
        let plaintext =
            serde_json::to_vec(document).map_err(|_| failure("cannot encode credentials"))?;
        let mut encrypted = cipher
            .encrypt(&iv.into(), plaintext.as_ref())
            .map_err(|_| failure("cannot encrypt credentials"))?;
        let tag = encrypted.split_off(encrypted.len() - 16);
        let encoded = format!(
            "{}:{}:{}",
            hex::encode(iv),
            hex::encode(tag),
            hex::encode(encrypted)
        );
        if encoded.len() as u64 > MAX_FILE_BYTES {
            return Err(failure("credential store is too large"));
        }
        self.atomic_write("credentials.json", encoded.as_bytes())
    }
    fn ensure_home(&self) -> Result<(), ProviderError> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&self.home)
            .map_err(|_| failure("cannot create credential home"))
    }
    fn lock(&self) -> Result<StoreLock, ProviderError> {
        let file = private_open(&self.home.join(".credentials.lock"), true)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(StoreLock(file)),
                Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => return Err(failure("cannot lock credential store")),
            }
        }
    }
    fn atomic_write(&self, name: &str, bytes: &[u8]) -> Result<(), ProviderError> {
        let target = self.home.join(name);
        if exists(&target)? {
            let _ = private_open(&target, false)?;
        }
        let mut random_name = [0u8; 16];
        random(&mut random_name)?;
        let temporary = self
            .home
            .join(format!(".credentials-{}.tmp", hex::encode(random_name)));
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
            }
            let mut file = options
                .open(&temporary)
                .map_err(|_| failure("cannot create credential file"))?;
            file.write_all(bytes)
                .and_then(|_| file.sync_all())
                .map_err(|_| failure("cannot write credentials"))?;
            drop(file);
            fs::rename(&temporary, &target).map_err(|_| failure("cannot replace credentials"))?;
            self.sync_home()
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
    fn sync_home(&self) -> Result<(), ProviderError> {
        #[cfg(unix)]
        {
            File::open(&self.home)
                .and_then(|file| file.sync_all())
                .map_err(|_| failure("cannot sync credential home"))?;
        }
        Ok(())
    }
}
struct StoreLock(File);
impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
fn failure(message: &str) -> ProviderError {
    ProviderError::new(ProviderErrorKind::CredentialStoreUnavailable, message)
}
fn random(bytes: &mut [u8]) -> Result<(), ProviderError> {
    getrandom::fill(bytes).map_err(|_| failure("credential randomness unavailable"))
}
fn decode(text: &str) -> Result<Vec<u8>, ProviderError> {
    hex::decode(text).map_err(|_| failure("invalid encrypted credentials"))
}
fn exists(path: &Path) -> Result<bool, ProviderError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(failure("cannot inspect credential file")),
    }
}
fn private_open(path: &Path, create: bool) -> Result<File, ProviderError> {
    if let Ok(meta) = fs::symlink_metadata(path)
        && !meta.is_file()
    {
        return Err(failure("credential path is not a regular file"));
    }
    let mut options = OpenOptions::new();
    options.read(true).write(create).create(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .map_err(|_| failure("cannot open credential file"))?;
    let meta = file
        .metadata()
        .map_err(|_| failure("cannot inspect credential file"))?;
    if !meta.is_file() {
        return Err(failure("credential path is not a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if meta.nlink() != 1 {
            return Err(failure("credential file has multiple links"));
        }
        if meta.permissions().mode() & 0o077 != 0 {
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| failure("cannot protect credential file"))?;
        }
    }
    Ok(file)
}
fn read_private(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, ProviderError> {
    if !exists(path)? {
        return Ok(None);
    }
    let file = private_open(path, false)?;
    let mut data = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|_| failure("cannot read credential file"))?;
    if data.len() as u64 > limit {
        return Err(failure("credential file is too large"));
    }
    Ok(Some(data))
}
#[cfg(test)]
mod tests;
