//! Edits use the same dotenv parser and protection policy as configuration loading.
use super::{HOME_TRUSTED_ENV_KEYS, PROTECTED_KEYS, parse_env};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// Set (`Some`) or remove (`None`) one global dotenv key; return whether bytes changed.
/// The caller passes the home resolved by the shared configuration layer.
pub fn edit_global_env(home: &Path, key: &str, value: Option<&str>) -> io::Result<bool> {
    if key.is_empty()
        || !key.bytes().enumerate().all(|(i, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (i > 0 && byte.is_ascii_digit())
        })
    {
        return Err(invalid("Key must match [A-Za-z_][A-Za-z0-9_]*."));
    }
    if value.is_some()
        && PROTECTED_KEYS.iter().any(|protected| {
            *protected == key || (cfg!(windows) && protected.eq_ignore_ascii_case(key))
        })
        && !HOME_TRUSTED_ENV_KEYS.contains(&key)
    {
        return Err(invalid(
            "This key cannot be loaded from a global .env; set it in the process environment.",
        ));
    }
    if value.is_some_and(|value| value.contains(['\n', '\r', '\0'])) {
        return Err(invalid(
            "Values must be a single line without NUL characters.",
        ));
    }
    if !home.exists() && value.is_none() {
        return Ok(false);
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(home)?;
    let lock_path = home.join(".env.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    reject_special_file(&lock_path)?;
    let lock = options.open(lock_path)?;
    lock.try_lock().map_err(|_| {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            "Global config is being edited; retry.",
        )
    })?;
    // Closing the file releases the OS lock, including on errors.
    let path = home.join(".env");
    reject_special_file(&path)?;
    let original = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut updated = String::new();
    let mut found = false;
    for line in original.split_inclusive('\n') {
        if parse_env(Some(line)).contains_key(key) {
            if !found && let Some(value) = value {
                updated.push_str(&format!("{key}=\"{value}\"{newline}"));
            }
            found = true;
        } else {
            updated.push_str(line);
        }
    }
    if !found && let Some(value) = value {
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push_str(newline);
        }
        updated.push_str(&format!("{key}=\"{value}\"{newline}"));
    }
    if updated == original {
        return Ok(false);
    }
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce)
        .map_err(|_| io::Error::other("Cannot create config temporary file name."))?;
    let temporary = home.join(format!(".env-{}.tmp", hex::encode(nonce)));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(updated.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &path)?;
        #[cfg(unix)]
        fs::File::open(home)?.sync_all()?;
        Ok(true)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn reject_special_file(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => Err(invalid(
            "Config files must be regular files, not symlinks or directories.",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
