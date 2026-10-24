//! `impact --since`: git changes, fsmonitor safety and empty diffs.
use super::*;

#[test]
fn impact_since_reads_git_and_widens_on_config_changes() {
    let dir = issues_fixture();
    let app = dir.0.as_path().join("app");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&app)
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if !git(&["init", "-q"]) {
        return; // git unavailable in this environment
    }
    assert!(git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "add",
        "-A"
    ]));
    assert!(git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "base"
    ]));
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    std::fs::write(
        app.join("src/fmt.ts"),
        "export const fmt = (s: string) => s;\n",
    )
    .expect("edit");
    let out = ask(dir.0.as_path(), "impact", None, |o| {
        o.since = Some("HEAD".into());
    });
    let files = impacted(&out);
    assert!(files.contains(&("src/fmt.ts".into(), 0)), "{}", out.value);
    assert!(files.contains(&("src/main.test.ts".into(), 1)), "{files:?}");
    assert!(!files.iter().any(|(f, _)| f == "src/lib.ts"));

    std::fs::write(
        app.join("package.json"),
        r#"{"name":"app","main":"dist/main.js"}"#,
    )
    .expect("manifest");
    let widened = ask(dir.0.as_path(), "impact", None, |o| {
        o.since = Some("HEAD".into());
    });
    assert!(
        widened.value["summary"]["configChanges"][0]["file"] == "package.json",
        "{}",
        widened.value["summary"]
    );
    assert!(
        impacted(&widened)
            .iter()
            .any(|(f, d)| f == "src/lib.ts" && *d == 0)
    );
}

/// `--since` runs git on a repository the caller controls; its config
/// must not run programs (an fsmonitor hook executes on every index read).
#[cfg(unix)]
#[test]
fn impact_since_does_not_run_repository_fsmonitor_hooks() {
    use std::os::unix::fs::PermissionsExt;
    let dir = issues_fixture();
    let app = dir.0.as_path().join("app");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&app)
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if !git(&["init", "-q"]) {
        return; // git unavailable in this environment
    }
    let identity = ["-c", "user.email=t@t", "-c", "user.name=t"];
    assert!(git(&[&identity[..], &["add", "-A"]].concat()));
    assert!(git(&[&identity[..], &["commit", "-qm", "base"]].concat()));
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let marker = dir.0.as_path().join("hook-ran");
    let hook = dir.0.as_path().join("fsmonitor.sh");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).expect("hook");
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    assert!(git(&[
        "config",
        "core.fsmonitor",
        hook.to_str().expect("utf-8 path")
    ]));
    std::fs::write(
        app.join("src/fmt.ts"),
        "export const fmt = (s: string) => s;\n",
    )
    .expect("edit");
    let out = ask(dir.0.as_path(), "impact", None, |o| {
        o.since = Some("HEAD".into());
    });
    assert!(
        impacted(&out).contains(&("src/fmt.ts".into(), 0)),
        "{}",
        out.value
    );
    assert!(!marker.exists(), "the repository fsmonitor hook ran");
}

/// `impact --since` with nothing changed is an empty answer (exit 1) that
/// says why, not an input error.
#[test]
fn impact_since_with_no_changes_is_empty() {
    let dir = fixture();
    let app = dir.0.as_path().join("app");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&app)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if !git(&["init", "-q"]) {
        return; // git unavailable in this environment
    }
    assert!(git(&["add", "-A"]) && git(&["commit", "-qm", "base"]));
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "impact", None, |o| {
        o.since = Some("HEAD".into());
    });
    assert_eq!(out.class, ExitClass::Empty, "{}", out.value);
    assert_eq!(out.value["total"], 0, "{}", out.value);
    assert_eq!(
        out.value["summary"]["reason"], "no files changed since HEAD",
        "{}",
        out.value
    );
    let bare = ask(dir.0.as_path(), "impact", None, |_| {});
    assert_eq!(bare.class, ExitClass::InvalidInput, "{}", bare.value);
}
