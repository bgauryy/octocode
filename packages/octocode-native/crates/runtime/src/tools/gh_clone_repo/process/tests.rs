//! Git process isolation: environment passthrough, credential handling and stderr scrubbing.
use super::*;
use std::collections::BTreeMap;

fn envs(command: &Command) -> BTreeMap<String, Option<String>> {
    command
        .get_envs()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.map(|value| value.to_string_lossy().into_owned()),
            )
        })
        .collect()
}

fn request() -> GitRunRequest<'static> {
    GitRunRequest {
        args: vec![OsString::from("--version")],
        timeout: Duration::from_secs(1),
        label: "test".into(),
        authorization: None,
        authorization_url: None,
    }
}

/// The isolated git keeps the runtime's proxy and CA settings (a
/// corporate network needs them) and its PATH, but nothing else.
#[test]
fn isolated_git_keeps_runtime_proxy_ca_and_path_only() {
    let runtime_env = BTreeMap::from(
        [
            ("HTTPS_PROXY", "http://proxy.corp:3128"),
            ("no_proxy", "internal.corp"),
            ("SSL_CERT_FILE", "/etc/corp-ca.pem"),
            ("GIT_SSL_CAINFO", "/etc/corp-ca.pem"),
            ("PATH", "/runtime/bin"),
            ("GIT_SSL_NO_VERIFY", "1"),
            ("GITHUB_TOKEN", "ghp_secret"),
        ]
        .map(|(key, value)| (key.to_owned(), value.to_owned())),
    );
    let git = SystemGit::with_env(&runtime_env);
    let envs = envs(&git.command(&request(), Path::new("/tmp/git-home"), None));
    let get = |key: &str| envs.get(key).cloned().flatten();
    assert_eq!(
        get("HTTPS_PROXY").as_deref(),
        Some("http://proxy.corp:3128")
    );
    assert_eq!(get("no_proxy").as_deref(), Some("internal.corp"));
    assert_eq!(get("SSL_CERT_FILE").as_deref(), Some("/etc/corp-ca.pem"));
    assert_eq!(get("GIT_SSL_CAINFO").as_deref(), Some("/etc/corp-ca.pem"));
    assert_eq!(get("PATH").as_deref(), Some("/runtime/bin"));
    assert_eq!(get("GIT_SSL_NO_VERIFY"), None);
    assert_eq!(get("GITHUB_TOKEN"), None);
}

/// The token reaches git only through an owner-only GIT_ASKPASS
/// helper that answers for the authorized origin alone. It is in neither
/// the child's environment nor its argv, and the helper is gone once git
/// exits.
#[cfg(unix)]
#[test]
fn the_token_reaches_git_only_through_an_owner_only_askpass_helper() {
    use std::os::unix::fs::PermissionsExt;
    let home = crate::tools::test_support::Fixture::new();
    let dump = home.0.join("dump");
    let script = home.0.join("git");
    std::fs::write(
        &script,
        format!(
            concat!(
                "#!/bin/sh\n{{\n",
                "env; printf 'arg:%s\\n' \"$@\"\n",
                "printf 'askpass:%s\\n' \"$GIT_ASKPASS\"\n",
                "ls -ld \"$GIT_ASKPASS\" \"$(dirname \"$GIT_ASKPASS\")\" | sed 's/^/mode:/'\n",
                "printf 'user:%s\\n' \"$(\"$GIT_ASKPASS\" \"Username for 'https://github.com': \")\"\n",
                "printf 'pass:%s\\n' \"$(\"$GIT_ASKPASS\" \"Password for 'https://x-access-token@github.com': \")\"\n",
                "printf 'other:%s\\n' \"$(\"$GIT_ASKPASS\" \"Password for 'https://x-access-token@evil.example': \")\"\n",
                "}} > '{}'\n"
            ),
            dump.display()
        ),
    )
    .expect("fixture git");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    let token = "ghp_secretToken123";
    // The fixture runs a dozen processes; a run its timeout ended under
    // load tested nothing and is repeated with a longer one.
    let timeouts = [10, 40].map(Duration::from_secs);
    crate::process_fixture::until_set_up(&timeouts, |timeout| {
        let ran = SystemGit::new(&script).run(
            &GitRunRequest {
                args: vec!["clone".into(), "https://github.com/owner/repo.git".into()],
                timeout,
                label: "askpass fixture".into(),
                authorization: Some(token),
                authorization_url: Some("https://github.com/owner/repo.git"),
            },
            &GitRunControl {
                cancellation: &crate::execution::cancel::NeverCancel,
                deadline: Instant::now() + timeout + Duration::from_secs(5),
                cache_home: &home.0,
            },
        );
        match ran {
            Err(error) if error.code == ErrorCode::Timeout => None,
            ran => Some(ran.expect("fixture runs")),
        }
    });
    let dump = std::fs::read_to_string(&dump).expect("dump");
    let (answers, child): (Vec<&str>, Vec<&str>) = dump.lines().partition(|line| {
        line.starts_with("user:") || line.starts_with("pass:") || line.starts_with("other:")
    });
    let child = child.join("\n");
    let environment = serde_json::json!(child);
    for secret in [token, basic_credential(token).as_str(), "GIT_CONFIG_VALUE"] {
        crate::tools::test_support::checks::assert_absent(&environment, secret);
    }
    assert_eq!(
        answers,
        ["user:x-access-token", &format!("pass:{token}"), "other:"],
        "the helper answers only for the authorized origin"
    );
    let modes: Vec<&str> = child
        .lines()
        .filter_map(|line| line.strip_prefix("mode:"))
        .collect();
    assert_eq!(modes.len(), 2, "{child}");
    assert!(
        modes
            .iter()
            .all(|mode| mode.get(1..10) == Some("rwx------")),
        "{modes:?}"
    );
    let helper = child
        .lines()
        .find_map(|line| line.strip_prefix("askpass:"))
        .expect("GIT_ASKPASS set");
    assert!(
        !Path::new(helper).exists(),
        "helper removed after git exits"
    );
}

#[cfg(unix)]
#[test]
fn output_limit_reports_the_failed_stage_without_suggesting_different_repo_inputs() {
    use std::os::unix::fs::PermissionsExt;
    let home = crate::tools::test_support::Fixture::new();
    let script = home.0.join("git");
    std::fs::write(
        &script,
        "#!/bin/sh\ndd if=/dev/zero bs=65536 count=2 2>/dev/null\n",
    )
    .expect("fixture git");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    // A small cap and a generous timeout: under load the write must hit
    // the cap, never race the request's own deadline.
    let error = SystemGit::new(&script)
        .with_max_output(64 * 1024)
        .run(
            &GitRunRequest {
                timeout: Duration::from_secs(10),
                ..request()
            },
            &GitRunControl {
                cancellation: &crate::execution::cancel::NeverCancel,
                deadline: Instant::now() + Duration::from_secs(10),
                cache_home: &home.0,
            },
        )
        .expect_err("collected process output remains bounded");
    assert_eq!(error.code, ErrorCode::GitFailed);
    assert!(error.message.contains("git test"), "{}", error.message);
    assert!(error.message.contains("output limit"), "{}", error.message);
    assert!(
        error
            .hints
            .iter()
            .any(|hint| hint.contains("process or resource"))
    );
    assert!(error.hints.iter().all(|hint| !hint.contains("owner/repo")));
}

/// Git may echo the injected header value without its
/// `Authorization:` prefix; the bare base64 form is reversible.
#[test]
fn scrub_redacts_the_bare_basic_credential() {
    let basic = basic_credential("ghp_token123");
    let text = format!("fatal: config value {basic} rejected; token ghp_token123");
    let scrubbed = scrub(&text, Some("ghp_token123"), Path::new("/nonexistent-home"));
    for secret in [basic.as_str(), "ghp_token123"] {
        crate::tools::test_support::checks::assert_absent(&serde_json::json!(scrubbed), secret);
    }
}
