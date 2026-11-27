//! Fixtures shared by the crate's unit tests.
use crate::http::{ClientPolicy, Proxy, client};

/// `policy` for a test against a loopback server: connect directly. Tests
/// build every client through [`loopback_client`], so none runs the operating
/// system's proxy lookup, a real system service (on macOS a
/// configuration-daemon session that first scans the executable's directory)
/// whose time follows the machine's load.
pub(crate) fn loopback(policy: ClientPolicy) -> ClientPolicy {
    ClientPolicy {
        proxy: Proxy::Direct,
        ..policy
    }
}

/// The client of [`loopback`]`(policy)`.
pub(crate) fn loopback_client(policy: ClientPolicy) -> reqwest::Client {
    client(&loopback(policy)).expect("loopback client")
}
