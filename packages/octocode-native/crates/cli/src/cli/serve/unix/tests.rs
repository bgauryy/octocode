use super::client::*;
use super::daemon::*;
use super::identity::*;
use super::sockets::*;
use super::*;
use crate::cli::Printed;
use crate::cli::serve::runs_on_server;
use octocode_native::id::ToolId;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
mod identity_keys;
mod requests;
use requests::exchange_reply;
mod server_lifecycle;
mod sockets;

static NO_ENV: BTreeMap<String, String> = BTreeMap::new();

fn request<'a>(input: &'a Value, cwd: &'a Path) -> Request<'a> {
    Request {
        tool: "localSearch",
        input,
        json_out: true,
        cwd,
        env: &NO_ENV,
        identity: "id",
    }
}

/// A synthetic configuration: `env` (with `OCTOCODE_HOME` = `home`) for
/// a CLI in `cwd`.
fn identity(
    home: &Path,
    cwd: &Path,
    env: &[(&str, &str)],
) -> (octocode_native::config::ConfigOutput, String) {
    use octocode_native::config::{RuntimeSurface, acquire_config_input, resolve_config};
    let mut vars: BTreeMap<String, String> = env
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    vars.entry("OCTOCODE_HOME".into())
        .or_insert_with(|| home.to_string_lossy().into_owned());
    let vars = served_env(vars);
    let input = acquire_config_input(
        vars.clone(),
        cwd.to_path_buf(),
        home.to_path_buf(),
        RuntimeSurface::Cli,
    );
    let config = resolve_config(&input);
    let identity = call_identity(&config, &vars);
    (config, identity)
}

/// A workspace with one source file, a private home, and the
/// environment both the server and the in-process call run with.
struct Fixture {
    home: tempfile::TempDir,
    repo: tempfile::TempDir,
    env: BTreeMap<String, String>,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().expect("home");
        let repo = tempfile::tempdir().expect("repo");
        std::fs::create_dir_all(repo.path().join(".git")).expect("git");
        for dir in ["a", "b"] {
            let dir = repo.path().join(dir);
            std::fs::create_dir_all(&dir).expect("dir");
            std::fs::write(
                dir.join("lib.rs"),
                "fn alpha() {}\nfn beta() { alpha(); }\nfn gamma() { alpha(); }\n",
            )
            .expect("source");
        }
        let env = BTreeMap::from([
            (
                "OCTOCODE_HOME".to_owned(),
                home.path().to_string_lossy().into_owned(),
            ),
            ("OCTOCODE_ENABLE_LOCAL".to_owned(), "true".to_owned()),
            ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
        ]);
        Self { home, repo, env }
    }

    fn dir(&self, name: &str) -> PathBuf {
        same_dir_form(&self.repo.path().join(name))
    }

    /// A directory, with the same source, whose runtime builds slowly
    /// ([`slow_build`]).
    fn slow_dir(&self) -> PathBuf {
        let slow = self.repo.path().join("slow");
        std::fs::create_dir_all(&slow).expect("slow");
        std::fs::copy(self.repo.path().join("a/lib.rs"), slow.join("lib.rs")).expect("source");
        same_dir_form(&slow)
    }

    /// The identity a client in `cwd` sends.
    fn identity(&self, cwd: &Path) -> String {
        use octocode_native::config::{RuntimeSurface, acquire_config_input, resolve_config};
        let os_home = octocode_io::env::home_dir().expect("home");
        let env = served_env(self.env.clone());
        let input =
            acquire_config_input(env.clone(), cwd.to_path_buf(), os_home, RuntimeSurface::Cli);
        call_identity(&resolve_config(&input), &env)
    }

    /// Send one call from `cwd`, with this fixture's environment and
    /// the identity a client there sends.
    async fn ask(
        &self,
        socket: &Path,
        cwd: &Path,
        tool: &str,
        input: &Value,
        json_out: bool,
    ) -> Result<Reply, Failed> {
        self.ask_as(socket, cwd, tool, input, json_out, &self.identity(cwd))
            .await
    }

    async fn ask_as(
        &self,
        socket: &Path,
        cwd: &Path,
        tool: &str,
        input: &Value,
        json_out: bool,
        identity: &str,
    ) -> Result<Reply, Failed> {
        let mut stream = connect(socket).await.expect("connect");
        let env = served_env(self.env.clone());
        let request = Request {
            tool,
            input,
            json_out,
            cwd,
            env: &env,
            identity,
        };
        exchange_reply(&mut stream, &request, REPLY_DEADLINE).await
    }

    /// The call as an in-process CLI in `cwd` prints it.
    async fn in_process(&self, cwd: &Path, tool: &str, input: Value, json_out: bool) -> Printed {
        let runtime = octocode_native::runtime::ToolRuntime::from_host(
            octocode_native::runtime::HostOptions {
                cwd: Some(cwd.to_path_buf()),
                env: Some(served_env(self.env.clone())),
                ..crate::cli::cli_host()
            },
        )
        .expect("runtime");
        let admission = runtime.admit("cli-1".into()).expect("admit");
        let query = input.clone();
        let result = runtime
            .execute(
                admission,
                tool.into(),
                input,
                octocode_native::runtime::Channel::Cli,
            )
            .await;
        runtime.close().await;
        Printed::finish(tool, result, json_out, &query, Some(cwd))
    }
}

/// The `slow` directories [`slow_build`] started a runtime build for.
static SLOW_BUILDS: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// The `slow` directories whose builds a test let finish
/// ([`finish_slow_builds`]), and the signal that wakes those builds.
static FINISHED_SLOW_BUILDS: (std::sync::Mutex<Vec<PathBuf>>, std::sync::Condvar) =
    (std::sync::Mutex::new(Vec::new()), std::sync::Condvar::new());

/// How long a slow build, or a test, waits for an event only a broken
/// server never brings, so the test fails instead of hanging.
const SLOW_BUILD_BOUND: Duration = Duration::from_secs(60);

fn slow_builds(dir: &Path) -> usize {
    let built = SLOW_BUILDS.lock().expect("builds");
    built.iter().filter(|built| *built == dir).count()
}

/// Let the builds of `dir` finish.
fn finish_slow_builds(dir: &Path) {
    let (finished, signal) = &FINISHED_SLOW_BUILDS;
    finished.lock().expect("finished").push(dir.to_path_buf());
    signal.notify_all();
}

/// Wait until `ready` holds, polling; panics past [`SLOW_BUILD_BOUND`].
async fn until(what: &str, ready: impl Fn() -> bool) {
    let started = std::time::Instant::now();
    while !ready() {
        assert!(started.elapsed() < SLOW_BUILD_BOUND, "never {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// A build that, in a directory named `slow`, runs until the test lets
/// it finish ([`finish_slow_builds`]).
fn slow_build(
    host: octocode_native::runtime::HostOptions,
    pool: std::sync::Arc<octocode_engine::lsp::pool::LspClientPool>,
) -> Result<octocode_native::runtime::ToolRuntime, octocode_native::runtime::RuntimeError> {
    if let Some(cwd) = host.cwd.as_ref().filter(|cwd| cwd.ends_with("slow")) {
        SLOW_BUILDS.lock().expect("builds").push(cwd.clone());
        let (finished, signal) = &FINISHED_SLOW_BUILDS;
        let finished = finished.lock().expect("finished");
        let (_finished, waited) = signal
            .wait_timeout_while(finished, SLOW_BUILD_BOUND, |finished| {
                !finished.contains(cwd)
            })
            .expect("finished");
        assert!(!waited.timed_out(), "no test let the {cwd:?} build finish");
    }
    octocode_native::runtime::ToolRuntime::from_host_with_lsp_pool(host, pool)
}
