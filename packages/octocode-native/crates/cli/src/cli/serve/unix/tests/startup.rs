//! Starting a server: the outcome a child announces, or how it ended,
//! decides the wait; no interval does.

use super::super::startup::*;
use super::*;

/// A fake server: `script` run by `sh`, with the startup channel and stderr
/// piped as [`spawn_server`] pipes them, and its stdin piped so a test can
/// hold it at `read` until it lets it go on.
fn fake_server(script: &str) -> tokio::process::Child {
    tokio::process::Command::new("/bin/sh")
        .args(["-c", script])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("fake server")
}

/// A cancel that never comes.
async fn never() {
    std::future::pending::<()>().await;
}

/// The window the removed connectivity poll gave a starting server
/// (400 connects 5 ms apart): what a failed start used to cost, and past
/// what a slow start used to be abandoned.
const OLD_POLL_WINDOW: Duration = Duration::from_secs(2);

#[test]
fn outcomes_round_trip_as_one_line() {
    for outcome in [
        Startup::Ready,
        Startup::Existing,
        Startup::Unavailable("cannot bind /x: Permission denied".into()),
    ] {
        let line = outcome.line();
        assert_eq!(line.matches('\n').count(), 1, "{line}");
        assert!(line.ends_with('\n'), "{line}");
        assert_eq!(Startup::parse(&line), Some(outcome));
    }
    assert_eq!(Startup::parse("{\"startup\":\"later\"}"), None);
    assert_eq!(Startup::parse("ready"), None);
}

/// A child that fails before it announces anything ends the wait as soon
/// as it exits, with its exit status and everything it said on stderr.
#[tokio::test]
async fn a_child_that_fails_before_it_is_ready_ends_the_wait_at_once_with_what_it_said() {
    let mut child =
        fake_server("echo 'cannot start: fixture failure' >&2; echo 'second line' >&2; exit 3");
    let started = std::time::Instant::now();
    let outcome = await_startup(&mut child, never()).await;
    let elapsed = started.elapsed();
    let Some(Startup::Unavailable(reason)) = outcome else {
        panic!("{outcome:?}");
    };
    assert!(reason.contains("exit status: 3"), "{reason}");
    assert!(
        reason.contains("cannot start: fixture failure second line"),
        "{reason}"
    );
    assert!(!reason.contains('\n'), "one line: {reason}");
    assert!(
        elapsed < OLD_POLL_WINDOW,
        "a dead child is not waited out: {elapsed:?}"
    );
}

/// A server that cannot serve says why on its channel, and that reason is
/// the one the client gets. A line that is no outcome is a failed start.
#[tokio::test]
async fn an_announced_outcome_decides_the_start() {
    let unavailable = Startup::Unavailable("cannot bind /s.sock: Permission denied".into());
    for (announced, expected) in [
        (Startup::Ready.line(), Startup::Ready),
        (Startup::Existing.line(), Startup::Existing),
        (unavailable.line(), unavailable),
    ] {
        // The announcing server stays alive: the outcome alone decides.
        let mut child = fake_server(&format!(
            "printf '%s' '{}'; read go",
            announced.replace('\'', "'\\''")
        ));
        assert_eq!(await_startup(&mut child, never()).await, Some(expected));
        drop(child.stdin.take());
        child.wait().await.expect("fake server exits");
    }
    let mut child = fake_server("echo 'not an outcome'");
    let outcome = await_startup(&mut child, never()).await;
    assert!(
        matches!(&outcome, Some(Startup::Unavailable(reason)) if reason.contains("not an outcome")),
        "{outcome:?}"
    );
    child.wait().await.expect("fake server exits");
}

/// A start that stays alive past the old polling window, then announces
/// it is ready, is a ready server: no interval abandons it.
#[tokio::test]
async fn a_slow_start_that_stays_alive_is_waited_for() {
    use tokio::io::AsyncWriteExt as _;
    let mut child = fake_server(&format!("read go; printf '%s' '{}'", Startup::Ready.line()));
    let mut gate = child.stdin.take().expect("stdin");
    let (outcome, ()) = tokio::join!(await_startup(&mut child, never()), async {
        tokio::time::sleep(OLD_POLL_WINDOW + Duration::from_millis(500)).await;
        gate.write_all(b"go\n").await.expect("let it start");
    });
    assert_eq!(outcome, Some(Startup::Ready));
    child.wait().await.expect("fake server exits");
}

/// Ctrl-C while a child starts ends the wait without an outcome and closes
/// the channel: the child, still unready, cannot announce on it.
#[tokio::test]
async fn cancelling_a_start_closes_the_channel() {
    use tokio::io::AsyncWriteExt as _;
    let mut child = fake_server(&format!(
        "read go; printf '%s' '{}' || exit 9; exit 0",
        Startup::Ready.line()
    ));
    let mut gate = child.stdin.take().expect("stdin");
    let cancel = tokio::time::sleep(Duration::from_millis(50));
    assert_eq!(await_startup(&mut child, cancel).await, None);
    gate.write_all(b"go\n").await.expect("let it announce");
    let status = child.wait().await.expect("fake server exits");
    assert!(!status.success(), "the announce failed: {status}");
}

/// The server's half: it claims an unwritable socket directory's socket as
/// unavailable, naming the socket and the cause.
#[test]
fn a_socket_that_cannot_be_bound_is_unavailable_with_its_cause() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("dir");
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).expect("locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).expect("chmod");
    let socket = locked.join("serve-x.sock");
    let claimed = claim(&socket);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    let Err(Startup::Unavailable(reason)) = claimed else {
        panic!("{:?}", claimed.map(|_| ()));
    };
    assert!(reason.contains("bind"), "{reason}");
    assert!(reason.contains(&socket.display().to_string()), "{reason}");
}

/// Leaves at `path` a stream socket's file that nobody answers on, as a
/// killed server leaves it: the socket is bound and closed and never listens.
/// A listener bound and dropped instead can stay answering: macOS marks a
/// socket close-on-exec only after `socket` returns, so a child another test
/// spawns in that gap keeps the listener open after the drop.
fn stale_socket(path: &Path) {
    use std::os::unix::ffi::OsStrExt as _;
    let name = path.as_os_str().as_bytes();
    // SAFETY: `sockaddr_un` is plain data, valid zeroed.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    assert!(name.len() < address.sun_path.len(), "{}", path.display());
    address.sun_family = libc::sa_family_t::try_from(libc::AF_UNIX).expect("family");
    for (slot, byte) in address.sun_path.iter_mut().zip(name) {
        *slot = libc::c_char::from_ne_bytes([*byte]);
    }
    let length =
        libc::socklen_t::try_from(std::mem::size_of::<libc::sockaddr_un>()).expect("length");
    // SAFETY: the descriptor is this function's from `socket` to `close`;
    // `bind` reads `length` bytes of `address`, which it holds.
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
        assert!(fd >= 0, "{}", std::io::Error::last_os_error());
        let bound = libc::bind(fd, (&raw const address).cast(), length);
        let error = std::io::Error::last_os_error();
        libc::close(fd);
        assert_eq!(bound, 0, "{error}");
    }
}

/// A socket file nobody answers on (its server was killed) is replaced;
/// one a server answers on is that server's.
#[test]
fn a_stale_socket_is_replaced_and_a_live_one_is_existing() {
    let dir = tempfile::tempdir().expect("dir");
    let socket = dir.path().join("serve-s.sock");
    stale_socket(&socket);
    assert!(socket.exists(), "a stale file");
    assert!(
        std::os::unix::net::UnixStream::connect(&socket).is_err(),
        "nobody answers on it"
    );
    let (listener, ours) = claim(&socket).expect("replaced");
    assert_eq!(socket_identity(&socket), ours);
    assert!(matches!(claim(&socket), Err(Startup::Existing)));
    drop(listener);
}

/// Servers that start at once for one socket: exactly one binds it, every
/// other one finds it answering, and none replaces it.
#[test]
fn servers_starting_at_once_leave_one_owner() {
    let dir = tempfile::tempdir().expect("dir");
    let socket = dir.path().join("serve-race.sock");
    let starters = 16;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(starters));
    let claims: Vec<_> = (0..starters)
        .map(|_| {
            let (socket, barrier) = (socket.clone(), std::sync::Arc::clone(&barrier));
            std::thread::spawn(move || {
                barrier.wait();
                claim(&socket)
            })
        })
        .collect();
    let claims: Vec<_> = claims
        .into_iter()
        .map(|claim| claim.join().expect("starter"))
        .collect();
    let owners: Vec<_> = claims
        .iter()
        .filter_map(|claim| claim.as_ref().ok())
        .collect();
    assert_eq!(owners.len(), 1, "one owner");
    assert!(
        claims
            .iter()
            .filter_map(|claim| claim.as_ref().err())
            .all(|outcome| *outcome == Startup::Existing),
        "the others found it answering"
    );
    assert_eq!(socket_identity(&socket), owners[0].1, "the owner's socket");
}

/// What a server announces decides its exit: 1 when it cannot serve, 0 when
/// another server answers; one whose ready announcement nobody took leaves
/// at once, with its socket, before it takes a call.
#[tokio::test]
async fn a_server_announces_its_outcome_before_it_serves() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("dir");
    let heard = std::cell::RefCell::new(Vec::new());
    let hear = |outcome: &Startup| {
        heard.borrow_mut().push(format!("{outcome:?}"));
        false
    };

    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).expect("locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).expect("chmod");
    let code = serve(locked.join("serve-x.sock"), hear).await;
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    assert_eq!(code, 1);

    let live = dir.path().join("serve-live.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&live).expect("bind");
    assert_eq!(serve(live.clone(), hear).await, 0);

    let gone = dir.path().join("serve-gone.sock");
    assert_eq!(serve(gone.clone(), hear).await, 1);
    assert!(!gone.exists(), "it took its socket with it");

    let heard = heard.into_inner();
    assert!(
        heard[0].starts_with("Unavailable(\"cannot bind "),
        "{heard:?}"
    );
    assert_eq!(heard[1..], ["Existing", "Ready"], "{heard:?}");
}
