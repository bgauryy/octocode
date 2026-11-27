use super::SendFailure;
use crate::http::ClientPolicy;
use crate::test_support::loopback_client;
use std::io::Write as _;

/// The failure of a GET of `url` through a loopback client, and its
/// error chain for the assertion message.
fn failure_of(url: String) -> (SendFailure, String) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let error = loopback_client(ClientPolicy::default())
            .get(url)
            .send()
            .await
            .expect_err("no answer");
        let mut chain = format!("{error:?}");
        let mut cause = std::error::Error::source(&error);
        while let Some(current) = cause {
            chain.push_str(&format!(" <- {current}"));
            cause = current.source();
        }
        (SendFailure::of(&error), chain)
    })
}

#[test]
fn a_closed_port_is_a_refused_connection() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    drop(listener);
    let (failure, chain) = failure_of(format!("http://{address}/"));
    assert_eq!(failure, SendFailure::Refused, "{chain}");
}

#[test]
fn a_peer_that_does_not_speak_tls_fails_the_handshake() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    let server = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n");
        }
    });
    let (failure, chain) = failure_of(format!("https://{address}/"));
    assert_eq!(failure, SendFailure::Tls, "{chain}");
    let _ = server.join();
}

/// Through a proxy, the client connects to the proxy: a refusal is the
/// proxy's, and the fix names the proxy settings.
#[test]
fn a_refusal_through_a_proxy_names_the_proxy_and_its_settings() {
    let proxy = Some("http://127.0.0.1:9");
    assert_eq!(
        SendFailure::Refused.what(proxy),
        "the proxy http://127.0.0.1:9 refused the connection"
    );
    assert!(SendFailure::Refused.fix(proxy).contains("HTTPS_PROXY"));
    assert!(SendFailure::Refused.fix(None).contains("reachable"));
    assert!(SendFailure::Tls.fix(None).contains("SSL_CERT_FILE"));
}
