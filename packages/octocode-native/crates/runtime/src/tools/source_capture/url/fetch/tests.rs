use super::*;

#[test]
fn one_internal_answer_denies_the_whole_host() {
    let host = Host::Domain("docs.example.org".to_owned());
    let mixed = vec![
        "93.184.216.34:443".parse().unwrap(),
        "10.0.0.5:443".parse().unwrap(),
    ];
    let error = public_addresses(&host, mixed).unwrap_err();
    assert_eq!(error.code, ErrorCode::SecurityValidationFailed);
    assert!(error.message.contains("10.0.0.5"), "{}", error.message);
    let error = public_addresses(&host, Vec::new()).unwrap_err();
    assert_eq!(error.code, ErrorCode::ClassificationContextFailed);
    let one = vec!["93.184.216.34:443".parse().unwrap()];
    assert_eq!(public_addresses(&host, one.clone()).unwrap(), one);
}

use std::net::TcpListener;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const NAME: &str = "docs.example.org";

/// `route` on `server`, addressed by a public-looking name pinned to the
/// loopback listener, as a real capture pins a name to its checked addresses.
fn pinned(server: &MockServer, route: &str) -> (Url, Host<String>, Vec<SocketAddr>) {
    let address = *server.address();
    (
        Url::parse(&format!("http://{NAME}:{}{route}", address.port())).unwrap(),
        Host::Domain(NAME.to_owned()),
        vec![address],
    )
}

fn budgeted(handle: &tokio::runtime::Handle, budget: RequestBudget) -> Network<'_> {
    Network {
        handle,
        accept: ACCEPTED,
        budget,
    }
}

async fn fetch_error(
    server: &MockServer,
    route: &str,
    budget: RequestBudget,
) -> ClassificationError {
    let handle = tokio::runtime::Handle::current();
    let (url, host, addresses) = pinned(server, route);
    budgeted(&handle, budget)
        .send(&url, &host, addresses)
        .await
        .err()
        .expect("the request fails")
}

fn budget(limit: usize) -> RequestBudget {
    RequestBudget::with_timeout(Duration::from_secs(5), limit)
}

#[tokio::test]
async fn a_reply_carries_status_headers_and_body_and_the_request_carries_no_identity() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("<p>hi</p>", "text/html"))
        .expect(1)
        .mount(&server)
        .await;
    let handle = tokio::runtime::Handle::current();
    let (url, host, addresses) = pinned(&server, "/page#section");
    let reply = budgeted(&handle, budget(MAX_BODY_BYTES))
        .send(&url, &host, addresses)
        .await
        .expect("a reply");
    assert_eq!(reply.status, 200);
    assert_eq!(reply.content_type.as_deref(), Some("text/html"));
    assert_eq!(reply.location, None);
    assert_eq!(reply.body, b"<p>hi</p>");
    let seen = server.received_requests().await.expect("recorded");
    let request = &seen[0];
    assert_eq!(
        request
            .headers
            .get("accept")
            .and_then(|value| value.to_str().ok()),
        Some(ACCEPTED)
    );
    let agent = request.headers.get("user-agent").expect("user agent");
    assert!(
        agent.to_str().unwrap().starts_with("octocode-native/"),
        "{agent:?}"
    );
    for name in ["cookie", "authorization"] {
        assert!(request.headers.get(name).is_none(), "{name} is never sent");
    }
    assert_eq!(
        request
            .headers
            .get("host")
            .and_then(|value| value.to_str().ok()),
        Some(format!("{NAME}:{}", server.address().port()).as_str()),
        "the name is kept for the Host header while the address is pinned"
    );
}

#[tokio::test]
async fn a_redirect_is_reported_not_followed_and_an_error_status_has_no_body() {
    let server = MockServer::builder().start().await;
    Mock::given(path("/old"))
        .respond_with(ResponseTemplate::new(307).insert_header("location", "/elsewhere"))
        .mount(&server)
        .await;
    Mock::given(path("/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_string("no such page"))
        .mount(&server)
        .await;
    let handle = tokio::runtime::Handle::current();
    let network = budgeted(&handle, budget(MAX_BODY_BYTES));
    let (url, host, addresses) = pinned(&server, "/old");
    let moved = network.send(&url, &host, addresses).await.ok().unwrap();
    assert_eq!(moved.status, 307);
    assert_eq!(moved.location.as_deref(), Some("/elsewhere"));
    assert!(moved.body.is_empty());
    let (url, host, addresses) = pinned(&server, "/missing");
    let missing = network.send(&url, &host, addresses).await.ok().unwrap();
    assert_eq!(missing.status, 404);
    assert!(missing.body.is_empty(), "an error page is never read");
    let requested = server.received_requests().await.unwrap();
    assert_eq!(requested.len(), 2, "the Location was not requested");
}

#[tokio::test]
async fn a_body_up_to_the_limit_is_read_and_one_byte_more_is_refused() {
    let server = MockServer::builder().start().await;
    Mock::given(path("/exact"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'a'; 1024]))
        .mount(&server)
        .await;
    Mock::given(path("/over"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'a'; MAX_BODY_BYTES + 1]))
        .mount(&server)
        .await;
    let handle = tokio::runtime::Handle::current();
    let (url, host, addresses) = pinned(&server, "/exact");
    let reply = budgeted(&handle, budget(1024))
        .send(&url, &host, addresses)
        .await
        .expect("a body at the limit fits");
    assert_eq!(reply.body.len(), 1024);
    let (url, host, addresses) = pinned(&server, "/exact");
    let error = budgeted(&handle, budget(1023))
        .send(&url, &host, addresses)
        .await
        .err()
        .expect("one byte over the limit");
    assert_eq!(error.code, ErrorCode::ClassificationContextTooLarge);
    let error = fetch_error(&server, "/over", budget(MAX_BODY_BYTES)).await;
    assert_eq!(error.code, ErrorCode::ClassificationContextTooLarge);
    assert!(
        error.message.contains("exceeds the 2 MiB capture limit"),
        "{}",
        error.message
    );
}

#[tokio::test]
async fn a_refused_connection_names_the_host_and_asks_for_the_text_as_state() {
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let handle = tokio::runtime::Handle::current();
    let url = Url::parse(&format!("http://{NAME}:{}/", closed.port())).unwrap();
    let error = budgeted(&handle, budget(MAX_BODY_BYTES))
        .send(&url, &Host::Domain(NAME.to_owned()), vec![closed])
        .await
        .err()
        .expect("nothing listens");
    assert_eq!(error.code, ErrorCode::ClassificationContextFailed);
    assert!(
        error
            .message
            .contains("The page at docs.example.org could not be fetched: the connection failed."),
        "{}",
        error.message
    );
}

#[tokio::test]
async fn a_slow_server_hits_the_deadline_and_a_cancelled_call_stops_first() {
    let server = MockServer::builder().start().await;
    Mock::given(path("/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;
    let short = RequestBudget::with_timeout(Duration::from_millis(200), MAX_BODY_BYTES);
    let error = fetch_error(&server, "/slow", short).await;
    assert_eq!(error.code, ErrorCode::Timeout);

    let cancelled = budget(MAX_BODY_BYTES);
    cancelled.cancellation.cancel();
    let error = fetch_error(&server, "/slow", cancelled).await;
    assert_eq!(error.code, ErrorCode::Cancelled);
}

#[tokio::test]
async fn a_url_without_a_host_is_denied_before_any_lookup() {
    let handle = tokio::runtime::Handle::current();
    let error = budgeted(&handle, budget(MAX_BODY_BYTES))
        .request(&Url::parse("data:text/plain,hi").unwrap())
        .await
        .err()
        .expect("no host");
    assert_eq!(error.code, ErrorCode::SecurityValidationFailed);
    assert_eq!(error.message, "The URL names no host.");
}

fn network(runtime: &tokio::runtime::Runtime) -> Network<'_> {
    Network {
        handle: runtime.handle(),
        accept: ACCEPTED,
        budget: RequestBudget::with_timeout(std::time::Duration::from_secs(5), MAX_BODY_BYTES),
    }
}

#[test]
fn a_loopback_or_private_host_is_refused_before_connecting() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    // Nothing listens on these ports: a connection attempt would fail
    // as classificationContextFailed, not as a denial.
    for address in [
        "https://127.0.0.1:9/",
        "https://[::1]:9/",
        "https://localhost:9/",
        "https://169.254.169.254/latest/meta-data/",
        "https://10.0.0.1:9/",
    ] {
        let error = network(&runtime)
            .get(&Url::parse(address).unwrap())
            .err()
            .unwrap();
        assert_eq!(
            error.code,
            ErrorCode::SecurityValidationFailed,
            "{address}: {error:?}"
        );
    }
}
