//! The per-user socket directory: socket paths, the live servers in it,
//! stopping them, and pruning past [`MAX_SERVERS`].

use std::path::{Path, PathBuf};

/// Servers one user keeps running in one socket directory.
pub(super) const MAX_SERVERS: usize = 4;

/// The request line that asks a server to stop.
pub(super) const STOP: &[u8] = b"{\"stop\":true}\n";

/// This user's socket directory under `temp`, and the user.
pub(super) fn socket_dir(temp: &Path) -> (PathBuf, u32) {
    // SAFETY: getuid has no preconditions.
    let uid = unsafe { libc::getuid() };
    (temp.join(format!("octocode-{uid}")), uid)
}

/// The socket for `key` in this user's private directory under `temp`, or
/// why it cannot be used. The length limit is the platform's own
/// (`sockaddr_un`, 104 bytes on macOS, 108 on Linux).
pub(super) fn socket_in(temp: &Path, key: &str) -> Result<PathBuf, String> {
    let (dir, uid) = socket_dir(temp);
    let socket = dir.join(format!("serve-{key}.sock"));
    if std::os::unix::net::SocketAddr::from_pathname(&socket).is_err() {
        return Err(format!("socket path too long: {}", socket.display()));
    }
    private_dir(&dir, uid)
        .ok_or_else(|| format!("{} is not a private directory of this user", dir.display()))?;
    Ok(socket)
}

/// A directory only user `uid` can enter, created when missing.
pub(super) fn private_dir(dir: &Path, uid: u32) -> Option<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    if !dir.exists() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .ok()?;
    }
    let meta = std::fs::symlink_metadata(dir).ok()?;
    (meta.is_dir() && meta.uid() == uid && meta.permissions().mode() & 0o077 == 0).then_some(())
}

/// Ask every server of this user to stop: a login or logout changes the
/// credential a server may still remember, so the next call starts a fresh
/// one. Each server finishes the calls it has.
pub(in crate::cli) fn stop_all() {
    let (dir, uid) = socket_dir(&std::env::temp_dir());
    if dir.exists() && private_dir(&dir, uid).is_some() {
        stop_in(&dir);
    }
}

/// Ask every live server with a socket in `dir` to stop.
pub(super) fn stop_in(dir: &Path) {
    for (_, stream) in live_servers(dir, None) {
        ask_to_stop(stream);
    }
}

/// The server sockets in `dir` other than `own`, each with a connection to
/// the server answering on it. A socket nobody answers on (its server was
/// killed before its idle exit) is removed.
pub(super) fn live_servers(
    dir: &Path,
    own: Option<&Path>,
) -> Vec<(PathBuf, std::os::unix::net::UnixStream)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut live = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_socket = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("serve-") && name.ends_with(".sock"));
        if !is_socket || own == Some(path.as_path()) {
            continue;
        }
        match std::os::unix::net::UnixStream::connect(&path) {
            Ok(stream) => live.push((path, stream)),
            Err(_) => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    live
}

/// Ask the server on `stream` to stop.
pub(super) fn ask_to_stop(mut stream: std::os::unix::net::UnixStream) {
    use std::io::Write as _;
    let _ = stream.write_all(STOP);
}

/// When a server starts: remove the sibling sockets nobody answers on, and
/// ask the least recently asked live servers past [`MAX_SERVERS`] (this one
/// counted) to stop.
pub(super) fn prune_siblings(own: &Path) {
    let Some(dir) = own.parent() else { return };
    let mut live = live_servers(dir, Some(own));
    live.sort_by_cached_key(|(path, _)| std::cmp::Reverse(last_asked(path)));
    for (_, stream) in live.into_iter().skip(MAX_SERVERS.saturating_sub(1)) {
        ask_to_stop(stream);
    }
}

/// When the server on `socket` was last asked anything ([`touch`]).
pub(super) fn last_asked(socket: &Path) -> Option<std::time::SystemTime> {
    std::fs::symlink_metadata(socket)
        .and_then(|meta| meta.modified())
        .ok()
}

/// Mark `socket` as just asked: its modification time orders servers for
/// [`prune_siblings`].
pub(super) fn touch(socket: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(socket.as_os_str().as_bytes()) else {
        return;
    };
    // SAFETY: `path` is NUL-terminated and outlives the call; null times
    // set both times to now.
    unsafe {
        libc::utimes(path.as_ptr(), std::ptr::null());
    }
}

/// Stop warm servers and require an acknowledgement after their work drains.
/// A failure leaves cache deletion to a later retry after the server stops.
pub(in crate::cli) async fn stop_all_and_wait() -> Result<(), String> {
    tokio::task::spawn_blocking(|| {
        use std::io::{BufRead, Read, Write};
        let (dir, uid) = socket_dir(&std::env::temp_dir());
        if !dir.exists() { return Ok(()); }
        private_dir(&dir, uid).ok_or_else(|| "warm server directory is not private".to_owned())?;
        let servers = live_servers(&dir, None);
        for (_, mut stream) in servers {
            let timeout = Some(super::client::REPLY_DEADLINE);
            stream.set_read_timeout(timeout).map_err(|error| error.to_string())?;
            stream.set_write_timeout(timeout).map_err(|error| error.to_string())?;
            stream.write_all(b"{\"stop\":true,\"drain\":true}\n").map_err(|error| error.to_string())?;
            let mut line = String::new();
            std::io::BufReader::new(stream).take(128).read_line(&mut line).map_err(|error| error.to_string())?;
            if line.trim() != "{\"stopped\":true}" {
                return Err("A warm server stopped without confirming that its calls drained; rerun cache clear after it exits.".into());
            }
        }
        Ok(())
    }).await.map_err(|error| error.to_string())?
}
