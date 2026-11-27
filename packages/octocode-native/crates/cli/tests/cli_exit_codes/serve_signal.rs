//! A warm server stopped by SIGTERM leaves like an idle one: it exits 0 and
//! its socket is gone.
use super::*;
use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::time::{Duration, Instant};

#[cfg(unix)]
#[test]
fn a_server_stopped_by_sigterm_exits_cleanly_and_removes_its_socket() {
    let workspace = Workspace::new();
    let persistent = workspace.persistent_cli();
    let socket = persistent.sockets().join("serve-signal.sock");
    let mut server = workspace
        .cli()
        .env("TMPDIR", persistent.sockets())
        .args(["serve", "--socket"])
        .arg(&socket)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the server");
    let mut announced = String::new();
    BufReader::new(server.stdout.take().expect("startup channel"))
        .read_line(&mut announced)
        .expect("startup line");
    let ready = announced.contains("ready") && socket.exists();
    let pid = server.id().to_string();
    if ready {
        let sent = Command::new("kill").args(["-TERM", &pid]).status();
        assert!(sent.is_ok_and(|status| status.success()), "SIGTERM sent");
    } else {
        let _ = server.kill();
    }
    // Only a stalled shutdown reaches this bound; a clean one takes moments.
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = server.try_wait().expect("poll the server") {
            break Some(status);
        }
        if Instant::now() > deadline {
            let _ = server.kill();
            let _ = server.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(ready, "the server announced ready: {announced:?}");
    let status = status.expect("the server exits after SIGTERM");
    assert_eq!(status.code(), Some(0), "SIGTERM is a clean exit");
    assert!(!socket.exists(), "the socket is removed");
}
