//! Why a request got no answer: the one reading of a transport error every
//! provider states (artifact registries, classification), so a failure names
//! its cause and the fix that cause takes instead of "retry later".
use std::error::Error as _;

/// What a request that got no answer failed at, read from its error chain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SendFailure {
    /// The client's own connect or read bound passed.
    Timeout,
    /// Nothing listens at the address the client connected to.
    Refused,
    /// The connection opened, then closed before an answer.
    Closed,
    /// The host name did not resolve.
    Dns,
    /// The TLS handshake failed: a peer that does not speak TLS, or a
    /// certificate the trust store does not accept.
    Tls,
    /// Another failure to open the connection.
    Connect,
    /// A failure after the connection opened that names no cause.
    Other,
}

/// The kind of `error`, or of the I/O error it wraps when it is a bare
/// `Other`: the connector wraps the TLS layer's `InvalidData` so, and an I/O
/// error's `source` skips the error it wraps.
fn innermost_kind(error: &std::io::Error) -> std::io::ErrorKind {
    let mut error = error;
    while error.kind() == std::io::ErrorKind::Other {
        match error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<std::io::Error>())
        {
            Some(inner) => error = inner,
            None => break,
        }
    }
    error.kind()
}

impl SendFailure {
    /// The cause of `error`, from the first typed cause in its chain.
    #[must_use]
    pub fn of(error: &reqwest::Error) -> Self {
        if error.is_timeout() {
            return Self::Timeout;
        }
        let mut cause = error.source();
        while let Some(current) = cause {
            if let Some(io) = current.downcast_ref::<std::io::Error>() {
                match innermost_kind(io) {
                    std::io::ErrorKind::ConnectionRefused => return Self::Refused,
                    std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::UnexpectedEof => return Self::Closed,
                    // rustls reports a failed handshake as invalid data.
                    std::io::ErrorKind::InvalidData if error.is_connect() => return Self::Tls,
                    std::io::ErrorKind::TimedOut => return Self::Timeout,
                    _ => {}
                }
            }
            // hyper-util's connector names a failed name lookup so.
            if current.to_string().starts_with("dns error") {
                return Self::Dns;
            }
            cause = current.source();
        }
        if error.is_connect() {
            Self::Connect
        } else {
            Self::Other
        }
    }

    /// What failed, as the clause after "failed: ", naming `proxy` when the
    /// request went through one: the client then connects to the proxy.
    #[must_use]
    pub fn what(self, proxy: Option<&str>) -> String {
        match (self, proxy) {
            (Self::Refused, Some(proxy)) => format!("the proxy {proxy} refused the connection"),
            (Self::Refused, None) => "the host refused the connection".to_owned(),
            (Self::Dns, Some(proxy)) => format!("the proxy {proxy} did not resolve (DNS)"),
            (Self::Dns, None) => "the host name did not resolve (DNS)".to_owned(),
            (Self::Tls, _) => "the TLS handshake failed".to_owned(),
            (Self::Timeout, _) => "it timed out".to_owned(),
            (Self::Closed, _) => "the connection closed before an answer".to_owned(),
            (Self::Connect, Some(proxy)) => {
                format!("no connection could be opened through the proxy {proxy}")
            }
            (Self::Connect, None) => "no connection could be opened".to_owned(),
            (Self::Other, _) => "the transport failed".to_owned(),
        }
    }

    /// The fix this cause takes.
    #[must_use]
    pub fn fix(self, proxy: Option<&str>) -> &'static str {
        match (self, proxy) {
            (Self::Refused | Self::Dns | Self::Connect, Some(_)) => {
                "Check HTTPS_PROXY, HTTP_PROXY, ALL_PROXY or the system proxy settings, or exempt the host with NO_PROXY."
            }
            (Self::Refused, None) => {
                "Check the configured URL and that its host is reachable from this machine."
            }
            (Self::Dns, None) => {
                "Check the host name in the configured URL, DNS and the network connection."
            }
            (Self::Tls, _) => {
                "If a proxy or firewall intercepts TLS, add its CA certificate with SSL_CERT_FILE or SSL_CERT_DIR; check that the URL serves HTTPS."
            }
            (Self::Connect, None) => {
                "Check the network connection, a firewall, and the configured URL."
            }
            (Self::Timeout | Self::Closed | Self::Other, _) => {
                "Retry later; if it repeats, check the network, proxy and firewall."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SendFailure;
    use crate::http::{ClientPolicy, loopback_client};
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
}
