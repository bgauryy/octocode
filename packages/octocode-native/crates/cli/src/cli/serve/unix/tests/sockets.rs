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
    remove_stale_sockets(&own);
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

/// A starting server never asks a live sibling to stop: how long a server
/// lives is its own idle expiry, which demand (calls) renews. A count
/// quota stopped a warm server, and its language servers, for another
/// agent's new one.
#[tokio::test]
async fn a_warm_server_survives_new_servers_starting() {
    let dir = tempfile::tempdir().expect("dir");
    // More servers than any plausible per-user count.
    let sockets: Vec<_> = (0..8)
        .map(|at| dir.path().join(format!("serve-{at}.sock")))
        .collect();
    tokio::task::LocalSet::new()
        .run_until(async {
            let mut servers = Vec::new();
            for socket in &sockets {
                servers.push(tokio::task::spawn_local(serve(socket.clone())));
                until("the server bound its socket", || socket.exists()).await;
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            // A stop request would have reached the earlier ones by now.
            tokio::time::sleep(Duration::from_millis(200)).await;
            for (socket, server) in sockets.iter().zip(&servers) {
                assert!(!server.is_finished(), "{} stopped", socket.display());
                assert!(socket.exists(), "{} removed", socket.display());
            }
            stop_in(dir.path());
            for server in servers {
                server.await.expect("server");
            }
        })
        .await;
}

/// A starting server removes only the sockets nobody answers on, and a
/// live server is not asked anything by it.
#[test]
fn a_starting_server_asks_live_siblings_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let own = dir.path().join("serve-own.sock");
    let live = dir.path().join("serve-live.sock");
    let listener = std::os::unix::net::UnixListener::bind(&live).expect("bind");
    remove_stale_sockets(&own);
    // The probe connection closes without a request, so a server reads end
    // of stream, not a stop.
    assert!(received(&listener).is_empty());
    // Stop requests go to every live server, and none is removed.
    stop_in(dir.path());
    assert_eq!(received(&listener), STOP);
    assert!(live.exists());
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

/// A warm server listening on `socket` that reads one request line and
/// answers `reply`; the request it read.
fn answering(socket: &std::path::Path, reply: &'static str) -> std::thread::JoinHandle<String> {
    use std::io::{BufRead as _, Write as _};
    let listener = std::os::unix::net::UnixListener::bind(socket).expect("bind");
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let mut request = String::new();
        std::io::BufReader::new(&stream)
            .read_line(&mut request)
            .expect("request");
        (&stream).write_all(reply.as_bytes()).expect("reply");
        request
    })
}

/// `cache clear` stops every warm server and goes on only when each
/// confirms its calls drained; no socket directory has nothing to stop, and
/// one another user could enter is refused.
#[test]
fn stopping_and_waiting_needs_each_servers_drain_confirmation() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let temp = tempfile::tempdir().expect("temp");
    assert_eq!(stop_and_wait_in(temp.path()), Ok(()), "no servers");
    let uid = std::fs::metadata(temp.path()).expect("meta").uid();
    let (dir, _) = socket_dir(temp.path());
    private_dir(&dir, uid).expect("private");

    let confirmed = answering(&dir.join("serve-a.sock"), "{\"stopped\":true}\n");
    assert_eq!(stop_and_wait_in(temp.path()), Ok(()));
    let request: serde_json::Value =
        serde_json::from_str(&confirmed.join().expect("server")).expect("JSON request");
    assert_eq!(request, json!({"stop": true, "drain": true}));
    std::fs::remove_file(dir.join("serve-a.sock")).expect("unbind");

    // B75 F4: a server with calls in flight says how many before it drains;
    // its confirmation still follows.
    let announced = answering(
        &dir.join("serve-c.sock"),
        "{\"draining\":2}\n{\"stopped\":true}\n",
    );
    assert_eq!(stop_and_wait_in(temp.path()), Ok(()));
    announced.join().expect("server");
    std::fs::remove_file(dir.join("serve-c.sock")).expect("unbind");
    assert_eq!(draining("{\"draining\":2}\n"), Some(2));
    assert_eq!(draining("{\"stopped\":true}\n"), None);

    let unconfirmed = answering(&dir.join("serve-b.sock"), "{\"stopped\":false}\n");
    assert!(stop_and_wait_in(temp.path()).is_err());
    unconfirmed.join().expect("server");
    std::fs::remove_file(dir.join("serve-b.sock")).expect("unbind");

    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    assert!(stop_and_wait_in(temp.path()).is_err(), "a shared directory");
}

#[test]
fn a_socket_that_cannot_be_restricted_is_an_error() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("dir");
    let socket = dir.path().join("serve-own.sock");
    assert!(restrict(&socket).is_err(), "no socket, no restriction");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind");
    restrict(&socket).expect("restrict");
    let mode = std::fs::metadata(&socket).expect("meta").permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}
