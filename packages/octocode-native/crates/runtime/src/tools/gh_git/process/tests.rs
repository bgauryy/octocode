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
        stdin: None,
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

/// The token reaches git only through an owner-only GIT_ASKPASS helper that
/// answers for the authorized origin alone. The token is never written to
/// disk: the helper is a fixed script that reads it from the environment of
/// this one git process tree, so a killed run leaves no credential behind.
/// It is never in argv either.
#[cfg(unix)]
#[test]
fn the_token_reaches_git_only_through_the_askpass_environment_never_disk_or_argv() {
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
                "sed 's/^/helper:/' \"$GIT_ASKPASS\"\n",
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
                stdin: None,
            },
            &GitRunControl {
                cancellation: &crate::execution::cancel::NeverCancel,
                deadline: Instant::now() + timeout + Duration::from_secs(5),
                home: &home.0,
            },
        );
        match ran {
            Err(error) if error.code == ErrorCode::Timeout => None,
            ran => Some(ran.expect("fixture runs")),
        }
    });
    let dumped = std::fs::read_to_string(&dump).expect("dump");
    let (answers, child): (Vec<&str>, Vec<&str>) = dumped.lines().partition(|line| {
        line.starts_with("user:") || line.starts_with("pass:") || line.starts_with("other:")
    });
    let child = child.join("\n");
    let argv = serde_json::json!(
        child
            .lines()
            .filter(|line| line.starts_with("arg:"))
            .collect::<Vec<_>>()
    );
    for secret in [token, basic_credential(token).as_str(), "GIT_CONFIG_VALUE"] {
        crate::tools::test_support::checks::assert_absent(&argv, secret);
    }
    // The helper git runs, read while git runs, holds no credential.
    let helper: Vec<&str> = child
        .lines()
        .filter(|line| line.starts_with("helper:"))
        .collect();
    assert!(!helper.is_empty(), "{child}");
    crate::tools::test_support::checks::assert_absent(&serde_json::json!(helper), token);
    // No file under the Octocode home holds the token, during or after the
    // run, and no per-run helper directory is left to sweep.
    let mut pending = vec![home.0.clone()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("read home") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path != dump && path != script {
                let bytes = std::fs::read(&path).expect("read file");
                assert!(
                    !String::from_utf8_lossy(&bytes).contains(token),
                    "{} holds the token",
                    path.display()
                );
            }
        }
    }
    let leftovers: Vec<_> = std::fs::read_dir(home.0.join("tmp"))
        .expect("tmp")
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.starts_with("askpass"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
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
}

/// The helper answers nothing when the prompts it may answer are not set:
/// an empty pattern must not match every prompt.
#[cfg(unix)]
#[test]
fn the_askpass_helper_answers_nothing_without_its_origin() {
    let home = crate::tools::test_support::Fixture::new();
    let helper = AskPass::create(&home.0, "ghp_secretToken123", "https://github.com/o/r.git")
        .expect("helper");
    for envs in [
        vec![],
        vec![("OCTOCODE_ASKPASS_TOKEN", "ghp_secretToken123")],
        vec![
            ("OCTOCODE_ASKPASS_USERNAME_PROMPT", ""),
            ("OCTOCODE_ASKPASS_PASSWORD_PROMPT", ""),
            ("OCTOCODE_ASKPASS_TOKEN", "ghp_secretToken123"),
        ],
    ] {
        let output = Command::new(&helper.script)
            .env_clear()
            .envs(envs.iter().copied())
            .arg("Password for 'https://x-access-token@evil.example': ")
            .output()
            .expect("run helper");
        assert!(!output.status.success(), "{envs:?}");
        assert!(output.stdout.is_empty(), "{envs:?}");
    }
}

fn allowed_protocols(command: &Command) -> Option<String> {
    envs(command).get("GIT_ALLOW_PROTOCOL").cloned().flatten()
}

/// Git may use HTTPS alone: for every remote, submodule and redirect.
/// Plain HTTP is added only when the authorized remote itself is loopback
/// HTTP (the endpoints `web_root` accepts); `file`, `ssh` and `ext` never.
#[test]
fn git_transports_are_https_plus_loopback_http_for_a_loopback_remote_only() {
    let git = SystemGit::with_env(&BTreeMap::new());
    let home = Path::new("/tmp/git-home");
    for (url, expected) in [
        (None, "https"),
        (Some("https://github.com/o/r.git"), "https"),
        (Some("https://ghe.corp:8443/o/r.git"), "https"),
        (Some("http://127.0.0.1:8080/o/r.git"), "https:http"),
        (Some("http://localhost/o/r.git"), "https:http"),
        (Some("http://[::1]:9/o/r.git"), "https:http"),
        (Some("http://ghe.corp/o/r.git"), "https"),
        (Some("file:///tmp/o/r.git"), "https"),
    ] {
        let request = GitRunRequest {
            authorization_url: url,
            stdin: None,
            ..request()
        };
        assert_eq!(
            allowed_protocols(&git.command(&request, home, None)).as_deref(),
            Some(expected),
            "{url:?}"
        );
    }
}

/// Real git refuses a transport outside the allow-list.
#[test]
fn real_git_refuses_a_transport_outside_the_allow_list() {
    let home = crate::tools::test_support::Fixture::new();
    let remote = url::Url::from_file_path(home.0.join("missing.git")).expect("file url");
    let error = SystemGit::with_env(&BTreeMap::new())
        .run(
            &GitRunRequest {
                args: vec!["ls-remote".into(), remote.as_str().into()],
                timeout: Duration::from_secs(20),
                ..request()
            },
            &GitRunControl {
                cancellation: &crate::execution::cancel::NeverCancel,
                deadline: Instant::now() + Duration::from_secs(25),
                home: &home.0,
            },
        )
        .expect_err("file transport refused");
    assert!(
        error.message.contains("transport 'file' not allowed"),
        "{}",
        error.message
    );
}

/// End to end with real git against a loopback server that challenges: git
/// hands its environment through `git-remote-http` to the helper, which
/// answers git's exact prompts for this origin (port included), so the
/// retried request carries the token as HTTP Basic.
#[cfg(unix)]
#[test]
fn real_git_sends_the_token_after_a_challenge_from_the_authorized_origin() {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || {
        let mut authorizations = Vec::new();
        for stream in listener.incoming().take(2) {
            let mut stream = stream.expect("connection");
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut authorization = None;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).expect("read") == 0 || line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("authorization")
                {
                    authorization = Some(value.trim().to_owned());
                }
            }
            let response = if authorization.is_some() {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            } else {
                "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"fixture\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            };
            stream.write_all(response.as_bytes()).expect("write");
            authorizations.push(authorization);
            if authorizations.last().is_some_and(Option::is_some) {
                break;
            }
        }
        authorizations
    });
    let home = crate::tools::test_support::Fixture::new();
    let url = format!("http://127.0.0.1:{port}/o/r.git");
    let token = "ghp_fixtureToken123";
    let ran = SystemGit::with_env(&BTreeMap::new()).run(
        &GitRunRequest {
            args: vec!["ls-remote".into(), url.clone().into()],
            timeout: Duration::from_secs(20),
            label: "challenge fixture".into(),
            authorization: Some(token),
            authorization_url: Some(&url),
            stdin: None,
        },
        &GitRunControl {
            cancellation: &crate::execution::cancel::NeverCancel,
            deadline: Instant::now() + Duration::from_secs(25),
            home: &home.0,
        },
    );
    let error = ran.expect_err("the fixture serves no repository");
    crate::tools::test_support::checks::assert_absent(&serde_json::json!(error.message), token);
    let authorizations = server.join().expect("server");
    assert_eq!(
        authorizations,
        [None, Some(format!("Basic {}", basic_credential(token)))],
        "{}",
        error.message
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
                home: &home.0,
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

/// A pid that no process owns: a child that has exited and been reaped.
#[cfg(unix)]
fn dead_pid() -> u32 {
    let mut child = Command::new("true").spawn().expect("spawn");
    let pid = child.id();
    child.wait().expect("wait");
    pid
}

/// The next git run removes the helper directories killed runs left, and
/// only those: a directory owned by a live process, one not named like this
/// tool's, and a symlink all stay.
#[cfg(unix)]
#[test]
fn the_next_run_sweeps_askpass_directories_of_dead_runs_only() {
    let home = crate::tools::test_support::Fixture::new();
    let nonce = "0123456789abcdef0123456789abcdef";
    let stale = home.0.join(format!("askpass-{}-{nonce}", dead_pid()));
    let live = home
        .0
        .join(format!("askpass-{}-{nonce}", std::process::id()));
    let mut sleeper = Command::new("sleep").arg("30").spawn().expect("sleep");
    let other_live = home.0.join(format!("askpass-{}-{nonce}", sleeper.id()));
    let unrelated = home.0.join("askpass-notes");
    let target = home.0.join("elsewhere");
    let link = home
        .0
        .join(format!("askpass-{}-{nonce}", dead_pid() + 1_000_000));
    for dir in [&stale, &live, &other_live, &unrelated, &target] {
        std::fs::create_dir(dir).expect("dir");
    }
    std::fs::write(stale.join("askpass"), "#!/bin/sh\n").expect("file");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");

    let helper = AskPass::create(&home.0, "ghp_secretToken123", "https://github.com/o/r.git")
        .expect("helper");
    let created = helper.script.parent().expect("dir").to_path_buf();
    sleeper.kill().expect("kill");
    sleeper.wait().expect("wait");

    assert!(!stale.exists(), "a dead run's directory is removed");
    assert!(live.exists() && other_live.exists(), "live runs stay");
    assert!(unrelated.exists() && target.exists() && link.exists());
    assert!(created.exists(), "the new helper stays until dropped");
}
