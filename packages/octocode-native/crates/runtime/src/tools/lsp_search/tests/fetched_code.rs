//! `lspSearch` starts no language server on code Octocode fetched, unless the
//! user's own settings say so. The server here is a script that records every
//! start: a marker file is the proof a call started one (and so could have run
//! the repository's build scripts), whatever the call's row says.
use crate::config::{ConfigInput, FileInput, RuntimeSurface};
use crate::execution::RuntimeLimits;
use crate::runtime::{Channel, ToolRuntime};
use crate::tools::gh_clone_repo::storage::clones_root;
use crate::tools::gh_structure::materialize::materialize_root;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

pub(super) struct World {
    _dir: tempfile::TempDir,
    home: PathBuf,
    os_home: PathBuf,
    pub(super) cwd: PathBuf,
    marker: PathBuf,
}

impl World {
    /// A home whose `lsp-servers.json` routes `.rs` to the recording server,
    /// and a working directory holding an ordinary checkout.
    pub(super) fn new() -> Self {
        Self::at(Path::new("home"), Path::new("home"), Path::new("work"))
    }

    /// A user running from `$HOME`, with the Octocode home at its default place.
    fn in_user_home() -> Self {
        let home = Path::new("user/.octocode");
        Self::at(home, Path::new("user"), Path::new("user"))
    }

    fn at(home: &Path, os_home: &Path, cwd: &Path) -> Self {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().canonicalize().expect("canonical");
        let (home, os_home, cwd) = (root.join(home), root.join(os_home), root.join(cwd));
        std::fs::create_dir_all(&home).expect("home");
        let marker = root.join("server-started");
        let server = root.join("recording-server");
        std::fs::write(
            &server,
            format!(
                "#!/bin/sh\necho started >> '{}'\nexit 1\n",
                marker.display()
            ),
        )
        .expect("server");
        std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
        std::fs::write(
            home.join("lsp-servers.json"),
            json!({"languageServers":{".rs":{"command":server,"args":[],"languageId":"rust"}}})
                .to_string(),
        )
        .expect("config");
        let world = Self {
            _dir: dir,
            home,
            os_home,
            cwd,
            marker,
        };
        project(&world.cwd.join("checkout"));
        world
    }

    fn clone_dir(&self) -> PathBuf {
        let dir = clones_root(&self.home).join("o/r/abc__host_x");
        project(&dir);
        dir
    }

    pub(super) fn runtime(
        &self,
        env: &[(&str, &str)],
        home_rc: &Value,
        workspace_rc: Option<&Value>,
    ) -> ToolRuntime {
        let mut vars = BTreeMap::from([(
            crate::config::environment::OCTOCODE_HOME.to_owned(),
            self.home.to_string_lossy().into_owned(),
        )]);
        vars.extend(env.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())));
        let project_rc = self.cwd.join(".octocode/.octocoderc");
        ToolRuntime::with_limits(
            ConfigInput {
                env: vars,
                cwd: self.cwd.clone(),
                os_home: self.os_home.clone(),
                global_env: FileInput::Missing {
                    path: self.home.join(".env"),
                },
                project_env: FileInput::Missing {
                    path: self.cwd.join(".octocode/.env"),
                },
                config_file: FileInput::Read {
                    path: self.home.join(".octocoderc"),
                    text: home_rc.to_string(),
                },
                project_config_file: match workspace_rc {
                    Some(rc) => FileInput::Read {
                        path: project_rc,
                        text: rc.to_string(),
                    },
                    None => FileInput::Missing { path: project_rc },
                },
                runtime_surface: RuntimeSurface::Mcp,
            },
            RuntimeLimits::DEFAULT,
        )
        .expect("runtime")
    }

    pub(super) fn started(&self) -> bool {
        self.marker.exists()
    }
}

/// A Rust project at `dir`.
fn project(dir: &Path) {
    std::fs::create_dir_all(dir.join("src")).expect("src");
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"a\"\n").expect("manifest");
    std::fs::write(dir.join("src/lib.rs"), "pub fn target() {}\n").expect("source");
}

pub(super) async fn call(runtime: &ToolRuntime, row: Value) -> Value {
    let admission = runtime.admit("fetched".into()).expect("admitted");
    let outcome = runtime
        .execute(
            admission,
            "lspSearch".into(),
            json!({"queries":[row]}),
            Channel::Mcp,
        )
        .await
        .expect("call");
    outcome.structured_content["results"][0]["data"].clone()
}

fn symbols(path: &Path) -> Value {
    json!({"path":path,"operation":"documentSymbols"})
}

/// The refusal names why, the tools that run nothing, and the opt-in, and no
/// call it offers is another `lspSearch`.
fn assert_refused(row: &Value, world: &World) {
    assert_eq!(row["errorCode"], "policy", "{row}");
    assert_eq!(row["lsp"]["serverAvailable"], false, "{row}");
    let error = row["error"].as_str().expect("error");
    let trust = crate::config::field::LSP_TRUST_FETCHED_CODE;
    for named in [
        "run code from the repository",
        "localSearch",
        "astSearch",
        "graphQuery",
        trust.path,
        trust.env_name().expect("bound"),
    ] {
        assert!(error.contains(named), "{named} in {error}");
    }
    assert!(!row.to_string().contains("\"tool\":\"lspSearch\""), "{row}");
    assert!(!world.started(), "a server started: {row}");
}

#[tokio::test]
async fn a_materialized_tree_starts_no_server() {
    let world = World::new();
    let tree = materialize_root(&world.home).join("o/r/abc");
    project(&tree);
    let runtime = world.runtime(&[], &json!({}), None);
    let row = call(&runtime, symbols(&tree.join("src/lib.rs"))).await;
    assert_refused(&row, &world);
}

#[tokio::test]
async fn a_clone_starts_no_server_and_leads_to_a_read_and_a_text_search() {
    let world = World::new();
    let clone = world.clone_dir();
    let runtime = world.runtime(&[], &json!({}), None);
    let row = call(&runtime, symbols(&clone.join("src/lib.rs"))).await;
    assert_refused(&row, &world);
    let anchored = call(
        &runtime,
        json!({"path":clone.join("src/lib.rs"),"operation":"references","symbolName":"target","line":1}),
    )
    .await;
    assert_refused(&anchored, &world);
    let leads = anchored["related"].to_string();
    assert!(leads.contains("localFetch"), "{anchored}");
    assert!(leads.contains("localSearch"), "{anchored}");
}

#[tokio::test]
async fn a_workspace_root_in_a_clone_starts_no_server() {
    let world = World::new();
    let clone = world.clone_dir();
    let runtime = world.runtime(&[], &json!({}), None);
    let root = json!({"workspaceRoot":clone,"operation":"workspaceSymbol","symbolQuery":"target"});
    assert_refused(&call(&runtime, root).await, &world);
    // The clones directory itself holds every clone.
    let all = json!({"workspaceRoot":clones_root(&world.home),"operation":"workspaceSymbol","symbolQuery":"target"});
    assert_refused(&call(&runtime, all).await, &world);
    // A directory path in the checkout does not launder a root in the clone.
    let mixed = json!({
        "path":world.cwd.join("checkout/src"),"workspaceRoot":clone,
        "operation":"workspaceSymbol","symbolQuery":"target"
    });
    assert_refused(&call(&runtime, mixed).await, &world);
}

/// A symlink in the checkout names the clone by another path: the resolved one
/// decides, for a file, a directory and a workspace root.
#[tokio::test]
async fn a_symlink_into_a_clone_starts_no_server() {
    let world = World::new();
    let clone = world.clone_dir();
    let (file, dir) = (world.cwd.join("linked.rs"), world.cwd.join("linked"));
    symlink(clone.join("src/lib.rs"), &file).expect("file link");
    symlink(&clone, &dir).expect("directory link");
    let runtime = world.runtime(&[], &json!({}), None);
    assert_refused(&call(&runtime, symbols(&file)).await, &world);
    assert_refused(
        &call(&runtime, symbols(&dir.join("src/lib.rs"))).await,
        &world,
    );
    let root = json!({"workspaceRoot":dir,"operation":"workspaceSymbol","symbolQuery":"target"});
    assert_refused(&call(&runtime, root).await, &world);
}

/// The repository being read must not be able to trust itself, even when it
/// is the directory the runtime runs in.
#[tokio::test]
async fn a_workspace_setting_cannot_opt_in() {
    let mut world = World::new();
    let clone = world.clone_dir();
    let widen = json!({"lsp":{"trustFetchedCode":true}});
    let from_checkout = world.runtime(&[], &json!({}), Some(&widen));
    let row = call(&from_checkout, symbols(&clone.join("src/lib.rs"))).await;
    assert_refused(&row, &world);
    world.cwd.clone_from(&clone);
    let from_clone = world.runtime(&[], &json!({}), Some(&widen));
    let row = call(&from_clone, symbols(Path::new("src/lib.rs"))).await;
    assert_refused(&row, &world);
}

#[tokio::test]
async fn a_checkout_keeps_its_server() {
    let world = World::new();
    world.clone_dir();
    let runtime = world.runtime(&[], &json!({}), None);
    let row = call(&runtime, symbols(&world.cwd.join("checkout/src/lib.rs"))).await;
    assert_ne!(row["errorCode"], "policy", "{row}");
    assert!(world.started(), "{row}");
}

#[tokio::test]
async fn the_home_setting_opts_in() {
    let world = World::new();
    let clone = world.clone_dir();
    let runtime = world.runtime(&[], &json!({"lsp":{"trustFetchedCode":true}}), None);
    let row = call(&runtime, symbols(&clone.join("src/lib.rs"))).await;
    assert_ne!(row["errorCode"], "policy", "{row}");
    assert!(world.started(), "{row}");
}

#[tokio::test]
async fn the_process_environment_opts_in() {
    let world = World::new();
    let clone = world.clone_dir();
    let env = crate::config::field::LSP_TRUST_FETCHED_CODE
        .env_name()
        .expect("bound");
    let runtime = world.runtime(&[(env, "true")], &json!({}), None);
    let row = call(&runtime, symbols(&clone.join("src/lib.rs"))).await;
    assert_ne!(row["errorCode"], "policy", "{row}");
    assert!(world.started(), "{row}");
}

fn symbols_at(path: &Path, root: &Path) -> Value {
    json!({"path":path,"workspaceRoot":root,"operation":"documentSymbols"})
}

/// A server enumerates what its root holds, dot directories included: csharp-ls
/// ran an MSBuild `<Exec>` and jdtls a `build.gradle` under `.octocode/clones`
/// from a root above it. So `$HOME` is refused once it holds fetched code, for
/// an explicit root and for one inferred from a stray manifest in `$HOME`.
#[tokio::test]
async fn a_root_holding_fetched_code_starts_no_server() {
    let world = World::in_user_home();
    world.clone_dir();
    project(&world.cwd.join("proj"));
    let file = world.cwd.join("proj/src/lib.rs");
    let runtime = world.runtime(&[], &json!({}), None);
    assert_refused(&call(&runtime, symbols_at(&file, &world.cwd)).await, &world);
    let root_only =
        json!({"workspaceRoot":world.cwd,"operation":"workspaceSymbol","symbolQuery":"target"});
    assert_refused(&call(&runtime, root_only).await, &world);
    std::fs::write(world.cwd.join("Cargo.toml"), "[workspace]\n").expect("stray manifest");
    std::fs::create_dir(world.cwd.join("scratch")).expect("scratch");
    let stray = world.cwd.join("scratch/a.rs");
    std::fs::write(&stray, "pub fn a() {}\n").expect("source");
    assert_refused(&call(&runtime, symbols(&stray)).await, &world);
}

/// What the refusal says to do works: a root that does not hold the home starts
/// a server, inferred or named, even with fetched code beside it.
#[tokio::test]
async fn a_project_under_home_keeps_its_server() {
    let inferred = World::in_user_home();
    inferred.clone_dir();
    project(&inferred.cwd.join("proj"));
    let runtime = inferred.runtime(&[], &json!({}), None);
    let row = call(&runtime, symbols(&inferred.cwd.join("proj/src/lib.rs"))).await;
    assert_ne!(row["errorCode"], "policy", "{row}");
    assert!(inferred.started(), "{row}");

    let named = World::in_user_home();
    named.clone_dir();
    std::fs::write(named.cwd.join("Cargo.toml"), "[workspace]\n").expect("stray manifest");
    let scratch = named.cwd.join("scratch");
    std::fs::create_dir(&scratch).expect("scratch");
    std::fs::write(scratch.join("a.rs"), "pub fn a() {}\n").expect("source");
    let runtime = named.runtime(&[], &json!({}), None);
    let row = call(&runtime, symbols_at(&scratch.join("a.rs"), &scratch)).await;
    assert_ne!(row["errorCode"], "policy", "{row}");
    assert!(named.started(), "{row}");
}

/// Until something is fetched, `$HOME` holds nothing a server could open.
#[tokio::test]
async fn home_keeps_its_server_before_anything_is_fetched() {
    let world = World::in_user_home();
    project(&world.cwd.join("proj"));
    let runtime = world.runtime(&[], &json!({}), None);
    let row = call(
        &runtime,
        symbols_at(&world.cwd.join("proj/src/lib.rs"), &world.cwd),
    )
    .await;
    assert_ne!(row["errorCode"], "policy", "{row}");
    assert!(world.started(), "{row}");
}

#[tokio::test]
async fn the_opt_in_covers_a_root_holding_fetched_code() {
    let world = World::in_user_home();
    world.clone_dir();
    project(&world.cwd.join("proj"));
    let opted = json!({"lsp":{"trustFetchedCode":true}});
    let runtime = world.runtime(&[], &opted, None);
    let row = call(
        &runtime,
        symbols_at(&world.cwd.join("proj/src/lib.rs"), &world.cwd),
    )
    .await;
    assert_ne!(row["errorCode"], "policy", "{row}");
    assert!(world.started(), "{row}");
}
