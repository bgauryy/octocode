//! A provider that takes the request and then sends nothing ends it at the
//! stall bound, once: the stall bound is the most one request waits on a
//! silent provider, whatever its retries. A provider still sending is never
//! cut, however many stall bounds its answer takes.
use super::*;
use crate::providers::classification::CIRCUIT_OPEN;
use tokio::net::TcpListener;

const STALL: Duration = Duration::from_millis(300);

/// The production budget of a call whose deadline is far away, with a test
/// stall bound in place of [`octocode_io::http::READ_STALL`].
fn stalling_budget() -> RequestBudget {
    budget(
        Instant::now() + Duration::from_secs(20),
        octocode_io::cancellation::Cancellation::new(),
    )
    .with_stall(STALL)
}

/// A loopback provider that accepts every connection, runs `answer` on it,
/// and counts the connections.
async fn provider<F, Fut>(accepted: Arc<AtomicUsize>, answer: F) -> String
where
    F: Fn(tokio::net::TcpStream) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            accepted.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(answer(socket));
        }
    });
    format!("http://{address}")
}

/// Read the request head, then hold the socket open without writing.
async fn read_then_hold(mut socket: tokio::net::TcpStream, reply: &'static [u8]) {
    let mut buffer = [0_u8; 4096];
    let _ = socket.read(&mut buffer).await;
    let _ = socket.write_all(reply).await;
    tokio::time::sleep(Duration::from_secs(60)).await;
    drop(socket);
}

async fn post_to(server: &str, budget: &RequestBudget) -> Result<Value, ClassificationError> {
    post(
        body(&json!({})),
        reqwest::header::HeaderMap::new(),
        &SecretString::from("test-key".to_owned()),
        endpoint(server, "v1/systemone").unwrap(),
        budget,
        3,
        &fresh_gate(4),
    )
    .await
    .map(|(body, _)| body)
}

/// The request ends once, at the stall bound, as a provider that could not
/// answer: a retry would wait on the same silent provider again.
async fn assert_ends_at_one_stall(reply: &'static [u8]) {
    let accepted = Arc::new(AtomicUsize::new(0));
    let server = provider(accepted.clone(), move |socket| {
        read_then_hold(socket, reply)
    })
    .await;
    let started = Instant::now();
    let error = post_to(&server, &stalling_budget()).await.unwrap_err();
    let elapsed = started.elapsed();
    assert_eq!(error.code, CIRCUIT_OPEN, "{error:?}");
    assert_eq!(error.provider_calls, 1, "a stall is not retried: {error:?}");
    assert_eq!(accepted.load(Ordering::SeqCst), 1, "one connection");
    assert!(elapsed >= STALL, "cut before the stall bound: {elapsed:?}");
    assert!(
        elapsed < STALL * 3,
        "the stall bound did not end the request: {elapsed:?} {error:?}"
    );
    assert!(
        error.message.contains("sent nothing"),
        "the message names the stall: {error:?}"
    );
    assert!(
        error.hints.iter().any(|hint| hint.contains("Retry")),
        "the error says how to retry: {error:?}"
    );
}

#[tokio::test]
async fn a_provider_that_accepts_and_never_answers_ends_the_request_at_one_stall() {
    assert_ends_at_one_stall(b"").await;
}

#[tokio::test]
async fn a_provider_that_stops_mid_body_ends_the_request_at_one_stall() {
    assert_ends_at_one_stall(
        b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 38\r\n\r\n{\"model\"",
    )
    .await;
}

/// A provider that thinks for most of a stall bound, then trickles its
/// answer a byte at a time, each inside the bound, is read whole over many
/// bounds.
#[tokio::test]
async fn a_provider_still_sending_is_never_cut_by_the_stall_bound() {
    let accepted = Arc::new(AtomicUsize::new(0));
    let server = provider(accepted.clone(), |mut socket| async move {
        let mut buffer = [0_u8; 4096];
        let _ = socket.read(&mut buffer).await;
        tokio::time::sleep(STALL * 2 / 3).await;
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            OK_BODY.len()
        );
        let _ = socket.write_all(head.as_bytes()).await;
        for chunk in OK_BODY.as_bytes().chunks(4) {
            tokio::time::sleep(STALL / 3).await;
            let _ = socket.write_all(chunk).await;
        }
        let _ = socket.shutdown().await;
    })
    .await;
    let started = Instant::now();
    let value = post_to(&server, &stalling_budget()).await.unwrap();
    assert_eq!(value["model"], "m");
    assert!(
        started.elapsed() > STALL * 3,
        "the answer spans several stall bounds"
    );
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
}

/// Production: a classification request has no deadline of its own, so its
/// budget's stall bound is [`octocode_io::http::READ_STALL`], and the client
/// sets no timeout that would end it a second way.
#[test]
fn a_provider_request_is_bounded_by_its_budget_stall_only() {
    let budget = budget(
        Instant::now() + Duration::from_secs(1),
        octocode_io::cancellation::Cancellation::new(),
    );
    assert_eq!(budget.stall(), Some(octocode_io::http::READ_STALL));
    for bypass_proxy in [false, true] {
        let policy = provider_policy(bypass_proxy);
        assert_eq!(policy.read_timeout, None);
        assert_eq!(policy.timeout, None);
    }
}
