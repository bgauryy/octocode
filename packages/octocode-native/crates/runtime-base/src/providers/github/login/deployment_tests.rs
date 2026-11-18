use super::LoginEndpoints;

/// A GHE.com enterprise logs in like github.com: its web and API hosts
/// are cloud hosts, its credentials stored for the web host.
#[test]
fn a_ghe_com_host_logs_in_at_its_cloud_api() {
    for host in ["acme.ghe.com", "api.acme.ghe.com"] {
        let login = LoginEndpoints::from_host(host);
        assert_eq!(login.api_origin, "https://api.acme.ghe.com", "{host}");
        assert_eq!(login.web_origin, "https://acme.ghe.com", "{host}");
        assert_eq!(login.host, "acme.ghe.com", "{host}");
    }
    let dotcom = LoginEndpoints::from_host("github.com");
    assert_eq!(
        (
            dotcom.web_origin.as_str(),
            dotcom.api_origin.as_str(),
            dotcom.host.as_str()
        ),
        ("https://github.com", "https://api.github.com", "github.com")
    );
    let server = LoginEndpoints::from_host("ghe.example.com");
    assert_eq!(server.api_origin, "https://ghe.example.com/api/v3");
    assert_eq!(server.host, "ghe.example.com");
}
