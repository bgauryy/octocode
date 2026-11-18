//! The MCP server and `--version` name the same build: the package version
//! with the tool contract's fingerprint as build metadata.
use super::server::Server;
use crate::support::Workspace;
use octocode_test_support::json_fixture;

#[test]
fn the_server_version_and_the_cli_version_carry_the_contract_fingerprint() {
    let fingerprint = octocode_contract_types::TOOL_TYPES_CONTRACT_FINGERPRINT;
    let metadata = format!("+{}", &fingerprint[..8]);
    let workspace = Workspace::new();

    let mut server = Server::start(&workspace);
    let init = server.request(
        "initialize",
        json_fixture(
            r#"{
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1"}
            }"#,
        ),
    );
    let served = init["result"]["serverInfo"]["version"]
        .as_str()
        .expect("serverInfo.version")
        .to_owned();
    assert!(served.ends_with(&metadata), "{served}");
    assert!(server.finish().success());

    let output = workspace
        .cli()
        .arg("--version")
        .output()
        .expect("--version");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("octocode {served}")
    );
}
