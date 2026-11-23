use super::*;
use crate::test_support::TmpDir;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A stand-in rust-analyzer: `--version` answers, `scip` records each run,
/// its environment and pids beside itself, then writes the output file (or,
/// when a `hang` file exists, waits with a child process until killed).
const FAKE: &str = r#"#!/bin/sh
d=$(dirname "$0")
case "$1" in
--version) cat "$d/version"; exit 0;;
scip)
  echo run >> "$d/runs"
  env > "$d/env.txt"
  echo $$ > "$d/pid"
  out=""
  while [ $# -gt 0 ]; do [ "$1" = "--output" ] && out="$2"; shift; done
  if [ -f "$d/hang" ]; then
    sleep 300 &
    echo $! > "$d/child"
    wait
  fi
  printf 'fake-scip' > "$out"
  exit 0;;
esac
exit 2
"#;

struct Fixture {
    tmp: TmpDir,
    options: LspDiscoveryOptions,
}

impl Fixture {
    /// A consenting user config over a fake rust-analyzer, and a cargo
    /// workspace at `ws`. `consent` selects what the config enables.
    fn new(consent: bool) -> Self {
        let tmp = TmpDir::new();
        let bin = tmp.join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        octocode_io::atomic::write_atomic_mode(
            &bin.join("rust-analyzer"),
            FAKE.as_bytes(),
            0o700,
            false,
        )
        .expect("fake server");
        std::fs::write(bin.join("version"), "rust-analyzer 1.0.0-fake\n").expect("version");
        let home = tmp.join("home");
        std::fs::create_dir_all(&home).expect("home");
        let enabled = json!({"enable": consent});
        let config = json!({"languageServers": {".rs": {
            "command": bin.join("rust-analyzer"), "languageId": "rust",
            "initializationOptions": {"cargo": {"buildScripts": enabled}, "procMacro": enabled},
        }}});
        std::fs::write(home.join("lsp-servers.json"), config.to_string()).expect("config");
        tmp.write("ws/Cargo.toml", "[workspace]\nmembers = [\"a\"]\n");
        tmp.write("ws/a/Cargo.toml", "[package]\nname = \"a\"\n");
        tmp.write("ws/a/src/lib.rs", "pub fn f() {}\n");
        let env = [("PATH", "/usr/bin:/bin"), ("SECRET_TOKEN", "hunter2")]
            .map(|(name, value)| (name.to_owned(), value.to_owned()));
        let options = LspDiscoveryOptions {
            octocode_home: Some(home),
            env: env.into_iter().collect(),
            ..LspDiscoveryOptions::default()
        };
        Self { tmp, options }
    }

    fn ws(&self) -> PathBuf {
        self.tmp.join("ws")
    }

    fn cache(&self) -> PathBuf {
        self.tmp.join("cache")
    }

    fn bin(&self, name: &str) -> PathBuf {
        self.tmp.join("bin").join(name)
    }

    fn runs(&self) -> usize {
        std::fs::read_to_string(self.bin("runs")).map_or(0, |text| text.lines().count())
    }

    fn plan(&self) -> ScipPlan {
        scip_plan(&self.ws().join("a/src"), &self.options).expect("plan")
    }
}

fn pid_alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid.trim()])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn stopping_after<T>(after: std::time::Duration, work: impl FnOnce() -> T) -> T {
    let stop = Arc::new(AtomicBool::new(false));
    let raise = Arc::clone(&stop);
    std::thread::spawn(move || {
        std::thread::sleep(after);
        raise.store(true, Ordering::Relaxed);
    });
    crate::deep_stack::while_stoppable(&stop, work)
}

#[test]
fn without_consent_nothing_is_planned_or_spawned() {
    let fixture = Fixture::new(false);
    let planned = scip_plan(&fixture.ws(), &fixture.options);
    assert!(
        matches!(planned, Err(ScipError::NotConsented)),
        "{planned:?}"
    );
    assert_eq!(fixture.runs(), 0);
}

#[test]
fn a_cargo_workspace_root_is_the_workspace_table_not_the_member() {
    let fixture = Fixture::new(true);
    assert_eq!(
        cargo_workspace_root(&fixture.ws().join("a/src")),
        Some(fixture.ws())
    );
    let lone = TmpDir::new();
    lone.write("Cargo.toml", "[package]\nname = \"x\"\n");
    lone.write("src/lib.rs", "");
    assert_eq!(
        cargo_workspace_root(&lone.join("src")),
        Some(lone.to_path_buf())
    );
}

#[cfg(unix)]
#[test]
fn the_run_writes_the_index_atomically_and_a_matching_key_reuses_it() {
    let fixture = Fixture::new(true);
    let plan = fixture.plan();
    let first = plan.index(&fixture.cache()).expect("index");
    assert!(!first.reused);
    assert_eq!(std::fs::read(&first.path).expect("index"), b"fake-scip");
    assert_eq!(
        first.path,
        fixture.cache().join(format!("{}.scip", plan.key))
    );
    assert_eq!(fixture.runs(), 1);
    let leftovers = std::fs::read_dir(fixture.cache())
        .expect("cache")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .count();
    assert_eq!(leftovers, 0, "the temp file is renamed away");

    let again = fixture.plan().index(&fixture.cache()).expect("index");
    assert!(again.reused);
    assert_eq!(fixture.runs(), 1, "a matching key runs nothing");
}

#[cfg(unix)]
#[test]
fn each_input_changes_the_key_and_runs_again() {
    let fixture = Fixture::new(true);
    let base = fixture.plan();
    base.index(&fixture.cache()).expect("index");
    let edits: [(&str, &str); 4] = [
        ("ws/a/src/lib.rs", "pub fn g() {}\n"),
        ("ws/a/Cargo.toml", "[package]\nname = \"b\"\n"),
        ("ws/Cargo.lock", "# lock\n"),
        ("ws/rust-toolchain.toml", "[toolchain]\n"),
    ];
    let mut keys = vec![base.key];
    for (path, content) in edits {
        fixture.tmp.write(path, content);
        let plan = fixture.plan();
        assert!(!keys.contains(&plan.key), "{path} must change the key");
        keys.push(plan.key);
    }
    // The server's version is an input too.
    std::fs::write(fixture.bin("version"), "rust-analyzer 2.0.0-fake\n").expect("version");
    assert!(!keys.contains(&fixture.plan().key));
    let before = fixture.runs();
    fixture.plan().index(&fixture.cache()).expect("index");
    assert_eq!(fixture.runs(), before + 1);
    // An unrelated file is not an input.
    let key = fixture.plan().key;
    fixture.tmp.write("ws/notes.txt", "x");
    fixture.tmp.write("ws/target/debug/x.rs", "x");
    assert_eq!(fixture.plan().key, key);
}

#[cfg(unix)]
#[test]
fn the_server_runs_under_the_filtered_environment() {
    let fixture = Fixture::new(true);
    fixture.plan().index(&fixture.cache()).expect("index");
    let env = std::fs::read_to_string(fixture.bin("env.txt")).expect("env");
    assert!(env.contains("PATH=/usr/bin:/bin"), "{env}");
    assert!(
        !env.contains("SECRET_TOKEN"),
        "a variable off the allowlist leaks: {env}"
    );
    // Only the call's own variables, the rustup guard and what `sh` itself sets.
    let names: Vec<&str> = env
        .lines()
        .filter_map(|line| line.split('=').next())
        .collect();
    let shell_own = ["PWD", "SHLVL", "_", "OLDPWD"];
    for name in names {
        assert!(
            ["PATH", "RUSTUP_AUTO_INSTALL"].contains(&name) || shell_own.contains(&name),
            "{name} reached the indexer from the process environment"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_stopped_caller_kills_the_indexer_tree_and_leaves_no_index() {
    let fixture = Fixture::new(true);
    std::fs::write(fixture.bin("hang"), "").expect("hang");
    let plan = fixture.plan();
    let stopped = stopping_after(std::time::Duration::from_millis(300), || {
        plan.index(&fixture.cache())
    });
    assert!(matches!(stopped, Err(ScipError::Stopped)), "{stopped:?}");
    let (pid, child) = (
        std::fs::read_to_string(fixture.bin("pid")).expect("pid"),
        std::fs::read_to_string(fixture.bin("child")).expect("child"),
    );
    let mut gone = false;
    for _ in 0..500 {
        gone = !pid_alive(&pid) && !pid_alive(&child);
        if gone {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(gone, "the indexer and its child die with the stop");
    assert!(!plan.cache_file(&fixture.cache()).exists());
}

#[cfg(unix)]
#[test]
fn a_failed_run_is_a_typed_error_and_a_later_build_can_try_again() {
    let fixture = Fixture::new(true);
    let plan = fixture.plan();
    std::fs::remove_file(fixture.bin("rust-analyzer")).expect("remove server");
    let failed = plan.index(&fixture.cache());
    assert!(matches!(failed, Err(ScipError::Run(_))), "{failed:?}");
    assert!(!plan.cache_file(&fixture.cache()).exists());
}

#[cfg(unix)]
#[test]
fn concurrent_builds_run_the_indexer_once() {
    let fixture = Fixture::new(true);
    let plans: Vec<ScipPlan> = (0..4).map(|_| fixture.plan()).collect();
    let cache = fixture.cache();
    let files: Vec<ScipFile> = std::thread::scope(|scope| {
        let workers: Vec<_> = plans
            .iter()
            .map(|plan| scope.spawn(|| plan.index(&cache).expect("index")))
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("join"))
            .collect()
    });
    assert_eq!(fixture.runs(), 1, "one run serves every chunk");
    assert_eq!(files.iter().filter(|file| !file.reused).count(), 1);
    assert!(files.iter().all(|file| file.path == files[0].path));
}
