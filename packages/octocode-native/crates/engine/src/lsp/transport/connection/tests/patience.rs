use super::*;

/// A fake CPU clock: `cpu_at(n)` is the server tree's cumulative CPU time at
/// the `n`th read.
fn cpu_clock(cpu_at: impl Fn(u64) -> Duration + Send + Sync + 'static) -> CpuClock {
    let reads = std::sync::atomic::AtomicU64::new(0);
    Arc::new(move || Some(cpu_at(reads.fetch_add(1, Ordering::Relaxed))))
}

fn connect_watched(
    cpu_clock: Option<CpuClock>,
    progress: Arc<ProgressTracker>,
) -> (JsonRpcConnection, Server) {
    let (client_w, server_r) = duplex(64 * 1024);
    let (server_w, client_r) = duplex(64 * 1024);
    let connection = JsonRpcConnection::new(client_r, client_w, context(), progress, cpu_clock);
    (
        connection,
        Server {
            reader: BufReader::new(server_r),
            writer: server_w,
        },
    )
}

/// Base timeout for the starvation tests: the policy derives its check step
/// and ceiling from it, so every scenario runs in virtual milliseconds.
const BASE_MS: u32 = 1_000;

/// Answers the first request after `delay` of (virtual) time.
fn answer_after(mut server: Server, delay: Duration) -> tokio::task::JoinHandle<Server> {
    tokio::spawn(async move {
        let request = server.read_message().await;
        tokio::time::sleep(delay).await;
        server
            .send(json!({"jsonrpc":"2.0","id":request["id"],"result":"late"}))
            .await;
        server
    })
}

/// A starved server (the machine is oversubscribed) answers after the base
/// timeout while its CPU time keeps growing: the request waits for it.
#[tokio::test(start_paused = true)]
async fn request_outlives_its_base_timeout_while_the_server_burns_cpu() {
    let (connection, server) =
        connect_watched(Some(cpu_clock(Duration::from_secs)), ProgressTracker::new());
    let _server = answer_after(server, Duration::from_millis(1_800));
    let started = Instant::now();
    let result = connection
        .request("textDocument/references", Value::Null, BASE_MS)
        .await
        .expect("a progressing server is waited for");
    assert_eq!(result, json!("late"));
    assert!(started.elapsed() >= Duration::from_millis(1_800));
    assert!(connection.is_alive());
}

/// A server squeezed to a few percent of a core (measured: ~5% of one under
/// heavy oversubscription) still shows steady CPU growth: that is progress.
#[tokio::test(start_paused = true)]
async fn a_server_squeezed_to_a_sliver_of_cpu_still_counts_as_progressing() {
    // 6 ms of CPU per 250 ms check step is 2.4% of a core.
    let (connection, server) = connect_watched(
        Some(cpu_clock(|read| Duration::from_millis(read * 6))),
        ProgressTracker::new(),
    );
    let _server = answer_after(server, Duration::from_millis(1_800));
    connection
        .request("textDocument/references", Value::Null, BASE_MS)
        .await
        .expect("a sliver of CPU is progress");
}

/// A server whose CPU time is flat is hung, not starved: the base timeout
/// stands, with the cancel and the keep-the-server policy unchanged.
#[tokio::test(start_paused = true)]
async fn request_times_out_at_the_base_when_the_server_shows_no_cpu() {
    let (connection, mut server) = connect_watched(
        Some(cpu_clock(|_| Duration::from_secs(5))),
        ProgressTracker::new(),
    );
    let started = Instant::now();
    let error = connection
        .request("textDocument/references", Value::Null, BASE_MS)
        .await
        .expect_err("a hung server times out");
    assert_eq!(error.kind(), &ErrorKind::Timeout);
    assert!(
        error.reason.contains("timed out after 1000ms"),
        "{}",
        error.reason
    );
    assert_eq!(started.elapsed(), Duration::from_millis(1_000));
    assert!(connection.is_alive(), "one timeout keeps the server");
    assert_eq!(server.read_message().await["id"], 1);
    assert_eq!(server.read_message().await["method"], "$/cancelRequest");
}

/// CPU growth that is noise (a few ms of timer ticks per check) is not
/// progress.
#[tokio::test(start_paused = true)]
async fn negligible_cpu_growth_is_not_progress() {
    let (connection, _server) = connect_watched(
        Some(cpu_clock(Duration::from_millis)),
        ProgressTracker::new(),
    );
    let started = Instant::now();
    let error = connection
        .request("textDocument/references", Value::Null, BASE_MS)
        .await
        .expect_err("idle server times out");
    assert_eq!(error.kind(), &ErrorKind::Timeout);
    assert_eq!(started.elapsed(), Duration::from_millis(1_000));
}

/// A server that never stops burning CPU is bounded by the ceiling.
#[tokio::test(start_paused = true)]
async fn request_stops_at_the_ceiling_while_progress_never_ends() {
    let (connection, _server) =
        connect_watched(Some(cpu_clock(Duration::from_secs)), ProgressTracker::new());
    let started = Instant::now();
    let error = connection
        .request("textDocument/references", Value::Null, BASE_MS)
        .await
        .expect_err("the ceiling is terminal");
    assert_eq!(error.kind(), &ErrorKind::Timeout);
    assert!(
        error.reason.contains("timed out after 2000ms"),
        "{}",
        error.reason
    );
    assert_eq!(started.elapsed(), Duration::from_millis(2_000));
}

/// The extension ends as soon as the progress does, before the ceiling.
#[tokio::test(start_paused = true)]
async fn request_times_out_once_cpu_progress_stalls() {
    let (connection, _server) = connect_watched(
        Some(cpu_clock(|read| Duration::from_secs(read.min(2)))),
        ProgressTracker::new(),
    );
    let started = Instant::now();
    let error = connection
        .request("textDocument/references", Value::Null, BASE_MS)
        .await
        .expect_err("progress stalled");
    assert_eq!(error.kind(), &ErrorKind::Timeout);
    assert_eq!(started.elapsed(), Duration::from_millis(1_500));
}

/// Attempts that share a [`WaitBudget`] (the content-modified retries of one
/// request) share its ceiling: the second attempt gets what the first left.
#[tokio::test(start_paused = true)]
async fn attempts_sharing_a_budget_share_its_ceiling() {
    let (connection, mut server) =
        connect_watched(Some(cpu_clock(Duration::from_secs)), ProgressTracker::new());
    let budget = WaitBudget::new(BASE_MS);
    let started = Instant::now();
    let first = tokio::spawn(async move {
        let request = server.read_message().await;
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        server
            .send(json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32801,"message":"content modified"}}))
            .await;
        server
    });
    connection
        .request("textDocument/references", Value::Null, budget)
        .await
        .expect_err("content modified");
    let _server = first.await.expect("server");
    let error = connection
        .request("textDocument/references", Value::Null, budget)
        .await
        .expect_err("the retry gets only the rest of the ceiling");
    assert_eq!(error.kind(), &ErrorKind::Timeout);
    assert_eq!(started.elapsed(), Duration::from_millis(2_000));
    assert!(
        error.reason.contains("timed out after 2000ms"),
        "the message counts the budget's whole wait, not the last attempt's: {}",
        error.reason
    );
}

/// An open `$/progress` token (rust-analyzer's "loading") is progress even
/// where no CPU clock exists; closing it ends the extension.
#[tokio::test(start_paused = true)]
async fn active_work_done_progress_extends_the_timeout() {
    let progress = ProgressTracker::new();
    progress.on_begin("indexing".to_owned()).await;
    let (connection, server) = connect_watched(None, Arc::clone(&progress));
    let _server = answer_after(server, Duration::from_millis(1_800));
    let result = connection
        .request("textDocument/references", Value::Null, BASE_MS)
        .await
        .expect("indexing server is waited for");
    assert_eq!(result, json!("late"));

    progress.on_end("indexing").await;
    let started = Instant::now();
    connection
        .request("textDocument/references", Value::Null, BASE_MS)
        .await
        .expect_err("no progress left");
    assert_eq!(started.elapsed(), Duration::from_millis(1_000));
}

/// Without a clock and without progress the policy is the plain base timeout.
#[tokio::test(start_paused = true)]
async fn no_clock_and_no_progress_times_out_at_the_base() {
    let (connection, _server) = connect_watched(None, ProgressTracker::new());
    let started = Instant::now();
    connection
        .request("textDocument/references", Value::Null, BASE_MS)
        .await
        .expect_err("times out");
    assert_eq!(started.elapsed(), Duration::from_millis(1_000));
}

/// Under a caller's deadline a starved server is waited for as long as it
/// progresses and the caller still has time: far past the fixed ceiling a
/// caller-less request keeps (twice the base).
#[tokio::test(start_paused = true)]
async fn a_progressing_server_is_waited_for_until_the_callers_deadline() {
    let (connection, server) =
        connect_watched(Some(cpu_clock(Duration::from_secs)), ProgressTracker::new());
    let _server = answer_after(server, Duration::from_millis(7_000));
    let started = Instant::now();
    let result = with_caller_deadline(
        started + Duration::from_secs(10),
        connection.request("textDocument/references", Value::Null, BASE_MS),
    )
    .await
    .expect("the caller still had time and the server progressed");
    assert_eq!(result, json!("late"));
    assert!(started.elapsed() >= Duration::from_millis(7_000));
}

/// A server that never stops progressing ends at the caller's deadline less
/// the reserve the caller needs to shape the answer.
#[tokio::test(start_paused = true)]
async fn a_request_never_waits_past_its_callers_deadline() {
    let (connection, _server) =
        connect_watched(Some(cpu_clock(Duration::from_secs)), ProgressTracker::new());
    let started = Instant::now();
    let caller_deadline = started + Duration::from_secs(6);
    let error = with_caller_deadline(
        caller_deadline,
        connection.request("textDocument/references", Value::Null, BASE_MS),
    )
    .await
    .expect_err("the caller's deadline is terminal");
    assert_eq!(error.kind(), &ErrorKind::Timeout);
    assert_eq!(Instant::now(), caller_deadline - DEADLINE_RESERVE);
}

/// A caller's deadline lends time only to a server that shows progress: a
/// hung one still fails at the base.
#[tokio::test(start_paused = true)]
async fn a_hung_server_still_times_out_at_the_base_under_a_callers_deadline() {
    let (connection, _server) = connect_watched(
        Some(cpu_clock(|_| Duration::from_secs(5))),
        ProgressTracker::new(),
    );
    let started = Instant::now();
    with_caller_deadline(
        started + Duration::from_secs(60),
        connection.request("textDocument/references", Value::Null, BASE_MS),
    )
    .await
    .expect_err("a hung server times out");
    assert_eq!(started.elapsed(), Duration::from_millis(1_000));
}

/// A caller with less time left than the base ends the wait at its deadline.
#[tokio::test(start_paused = true)]
async fn a_callers_deadline_inside_the_base_cuts_the_wait() {
    let (connection, _server) = connect_watched(None, ProgressTracker::new());
    let started = Instant::now();
    let caller_deadline = started + Duration::from_millis(1_500);
    with_caller_deadline(
        caller_deadline,
        connection.request("textDocument/references", Value::Null, 5 * BASE_MS),
    )
    .await
    .expect_err("the caller ran out of time");
    assert_eq!(Instant::now(), caller_deadline - DEADLINE_RESERVE);
}

/// Dropping the request future during an extension still cancels the request.
#[tokio::test(start_paused = true)]
async fn dropping_an_extended_request_still_sends_cancel() {
    let (connection, mut server) =
        connect_watched(Some(cpu_clock(Duration::from_secs)), ProgressTracker::new());
    let dropped = tokio::time::timeout(
        Duration::from_millis(1_500),
        connection.request("textDocument/references", Value::Null, BASE_MS),
    )
    .await;
    assert!(dropped.is_err(), "the caller gave up during the extension");
    assert_eq!(server.read_message().await["id"], 1);
    let cancel = server.read_message().await;
    assert_eq!(cancel["method"], "$/cancelRequest");
    assert_eq!(cancel["params"]["id"], 1);
    assert_eq!(pending_len(&connection), Some(0));
}
