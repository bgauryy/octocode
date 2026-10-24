//! Observe the environment of a real LSP child without mutating the test runner.
use super::*;
use serde_json::Value;

const CHILD: &str = "OCTOCODE_LSP_ENV_FIXTURE";
const TOOLCHAIN_ENV: &[&str] = &["RUSTUP_HOME", "CARGO_HOME", "RUSTUP_TOOLCHAIN"];
const CREDENTIALS: &[&str] = &[
    "GITHUB_TOKEN",
    "AWS_SECRET_ACCESS_KEY",
    "CARGO_REGISTRIES_PRIVATE_TOKEN",
];

#[test]
fn toolchain_environment_reaches_the_server_without_inheriting_credentials() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(async {
        if std::env::var_os(CHILD).is_some() {
            check_server_environment().await;
            return;
        }
        let home = tempfile::tempdir().expect("isolated home");
        let test = module_path!()
            .split_once("::")
            .map(|(_, path)| format!("{path}::toolchain_environment_reaches_the_server_without_inheriting_credentials"))
            .expect("test module");
        let mut child = tokio::process::Command::new(std::env::current_exe().expect("test binary"));
        child
            .args(["--exact", &test, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env("RUSTUP_HOME", home.path().join("toolchains"))
            .env("CARGO_HOME", home.path().join("cargo"))
            .env("RUSTUP_TOOLCHAIN", "fixture-toolchain")
            .kill_on_drop(true);
        for key in CREDENTIALS {
            child.env(key, "fixture-credential-must-not-reach-server");
        }
        let output = tokio::time::timeout(Duration::from_secs(30), child.output())
            .await
            .expect("bounded child test")
            .expect("spawn child test");
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    });
}

async fn check_server_environment() {
    for override_environment in [false, true] {
        let (_tmp, root, mut config) = configurable_server("lsp-env", &[], None);
        let report = root.join("observed-env.json");
        let wrapper = root.join("capture-env.js");
        std::fs::write(
            &wrapper,
            r#"#!/usr/bin/env node
const fs = require('fs');
fs.writeFileSync(process.argv[3], JSON.stringify(process.env));
require(process.argv[2]);
"#,
        )
        .expect("capture fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755))
                .expect("fixture executable");
        }
        config.args = Some(vec![config.command, report.to_string_lossy().into_owned()]);
        config.command = wrapper.to_string_lossy().into_owned();
        let mut expected = TOOLCHAIN_ENV
            .iter()
            .map(|key| {
                (
                    (*key).to_owned(),
                    std::env::var(key).expect("isolated toolchain setting"),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        if override_environment {
            for (key, value) in &mut expected {
                *value = format!("configured-{key}");
            }
            config.env = Some(expected.clone());
        }
        let client = NativeLspClient::new(config);
        client.start().await.expect("start environment fixture");
        client.stop().await.expect("stop environment fixture");
        let observed: Value =
            serde_json::from_slice(&std::fs::read(&report).expect("server environment"))
                .expect("environment JSON");
        for (key, value) in expected {
            assert_eq!(
                observed[&key], value,
                "server environment for {key}, override={override_environment}"
            );
        }
        for key in CREDENTIALS.iter().chain(&[CHILD]) {
            assert!(
                observed.get(*key).is_none(),
                "unrequested {key} reached the server"
            );
        }
    }
}
