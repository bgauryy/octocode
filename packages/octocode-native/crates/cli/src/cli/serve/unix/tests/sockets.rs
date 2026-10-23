use super::*;

#[test]
fn the_least_recently_used_runtime_leaves_at_the_cap() {
    let mut recent = Recent::new(2);
    assert_eq!(recent.insert("a", 1), None);
    assert_eq!(recent.insert("b", 2), None);
    assert_eq!(recent.get(&"a"), Some(&1));
    // `b` is now the least recently used.
    assert_eq!(recent.insert("c", 3), Some(2));
    assert_eq!(recent.get(&"b"), None);
    // Replacing a key keeps one entry for it.
    assert_eq!(recent.insert("a", 4), None);
    assert_eq!(recent.get(&"a"), Some(&4));
    assert_eq!(recent.drain(), vec![3, 4]);
}

#[test]
fn stale_sockets_are_pruned_and_live_ones_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let own = dir.path().join("serve-own.sock");
    let live = dir.path().join("serve-live.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&live).expect("bind");
    let stale = dir.path().join("serve-stale.sock");
    drop(std::os::unix::net::UnixListener::bind(&stale).expect("bind"));
    let other = dir.path().join("notes.txt");
    std::fs::write(&other, "x").expect("write");
    prune_siblings(&own);
    assert!(live.exists() && other.exists());
    assert!(!stale.exists());
}

/// What a server listening on `listener` was sent by one connection.
fn received(listener: &std::os::unix::net::UnixListener) -> Vec<u8> {
    use std::io::Read as _;
    let (mut stream, _) = listener.accept().expect("accept");
    let mut sent = Vec::new();
    stream.read_to_end(&mut sent).expect("read");
    sent
}

/// A starting server keeps [`MAX_SERVERS`] in all: the least recently
/// asked live ones past the cap are asked to stop, the others are left
/// alone.
#[test]
fn a_starting_server_stops_the_least_recently_asked_past_the_cap() {
    let dir = tempfile::tempdir().expect("dir");
    let own = dir.path().join("serve-own.sock");
    let servers: Vec<_> = (0..=MAX_SERVERS)
        .map(|at| {
            let path = dir.path().join(format!("serve-{at}.sock"));
            let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
            std::thread::sleep(Duration::from_millis(20));
            (path, listener)
        })
        .collect();
    // The first one bound was asked last, so it is the most recent.
    touch(&servers[0].0);
    prune_siblings(&own);
    // Kept: the first, then the newest bound, up to the cap with `own`;
    // asked to stop: the oldest bound after the first.
    let stopped = servers.len() - (MAX_SERVERS - 1);
    for (at, (path, listener)) in servers.iter().enumerate() {
        let asked_to_stop = (1..=stopped).contains(&at);
        assert_eq!(
            received(listener) == STOP,
            asked_to_stop,
            "{}",
            path.display()
        );
    }
    // Stop requests go to every live server, and none is removed.
    stop_in(dir.path());
    for (path, listener) in &servers {
        assert_eq!(received(listener), STOP, "{}", path.display());
        assert!(path.exists());
    }
}

/// The socket's place does not depend on the Octocode home, so a
/// long home (a macOS `$TMPDIR` one) still gets a warm server; a
/// location too long for a socket says so instead of being skipped.
#[test]
fn the_socket_lives_under_the_temp_dir_whatever_the_home() {
    let temp = tempfile::tempdir().expect("temp");
    let socket = socket_in(temp.path(), "0123456789abcdef").expect("socket");
    assert!(socket.starts_with(temp.path()), "{}", socket.display());
    assert!(
        std::os::unix::net::SocketAddr::from_pathname(&socket).is_ok(),
        "{}",
        socket.display()
    );
    let deep = temp.path().join("d".repeat(120));
    std::fs::create_dir_all(&deep).expect("deep");
    let refused = socket_in(&deep, "0123456789abcdef").expect_err("too long");
    assert!(refused.contains("too long"), "{refused}");
}

#[test]
fn socket_directory_must_be_private() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let temp = tempfile::tempdir().expect("temp");
    let uid = std::fs::metadata(temp.path()).expect("meta").uid();
    let sockets = temp.path().join(format!("octocode-{uid}"));
    assert!(private_dir(&sockets, uid).is_some());
    assert!(private_dir(&sockets, uid + 1).is_none());
    std::fs::set_permissions(&sockets, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    assert!(private_dir(&sockets, uid).is_none());
}
