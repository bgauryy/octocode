//! `lspSearch` runs a Rust workspace's build scripts and proc macros (repository
//! code) only when the host's own settings allow it. The server here is a
//! script that records every start: a marker file is the proof a call started
//! one, whatever the call's row says.
use super::fetched_code::{World, call};
use serde_json::{Value, json};

fn query(world: &World, context: Value) -> Value {
    json!({
        "path": world.cwd.join("checkout/src/lib.rs"),
        "operation": "documentSymbols",
        "rustContext": context,
    })
}

/// The refusal names what it would run and the exact opt-in, offers no
/// `lspSearch` lead (it would be refused too), and starts no server.
fn assert_refused(row: &Value, world: &World) {
    assert_eq!(row["errorCode"], "policy", "{row}");
    assert_eq!(row["lsp"]["serverAvailable"], false, "{row}");
    let error = row["error"].as_str().expect("error");
    let allow = crate::config::field::LSP_ALLOW_BUILD_SCRIPTS;
    for named in [
        "build scripts",
        "procedural macros",
        "repository code",
        allow.path,
        allow.env_name().expect("bound"),
    ] {
        assert!(error.contains(named), "{named} in {error}");
    }
    assert!(!row.to_string().contains("\"tool\":\"lspSearch\""), "{row}");
    assert!(!world.started(), "a server started: {row}");
}

fn assert_answers(row: &Value, world: &World) {
    assert_ne!(row["errorCode"], "policy", "{row}");
    assert!(world.started(), "{row}");
}

#[tokio::test]
async fn build_scripts_are_refused_by_default() {
    let world = World::new();
    let runtime = world.runtime(&[], &json!({}), None);
    let row = call(&runtime, query(&world, json!({"buildScripts": true}))).await;
    assert_refused(&row, &world);
}

#[tokio::test]
async fn proc_macros_are_refused_by_default() {
    let world = World::new();
    let runtime = world.runtime(&[], &json!({}), None);
    let alone = call(&runtime, query(&world, json!({"procMacros": true}))).await;
    assert_refused(&alone, &world);
    let both = json!({"buildScripts": true, "procMacros": true});
    assert_refused(&call(&runtime, query(&world, both)).await, &world);
}

/// A refused context leaves the default server for every other query, and an
/// explicit `false` or an unrelated context key is no request to run code.
#[tokio::test]
async fn other_queries_keep_the_default_server() {
    let world = World::new();
    let runtime = world.runtime(&[], &json!({}), None);
    assert_refused(
        &call(&runtime, query(&world, json!({"buildScripts": true}))).await,
        &world,
    );
    let plain =
        json!({"path": world.cwd.join("checkout/src/lib.rs"), "operation": "documentSymbols"});
    assert_answers(&call(&runtime, plain).await, &world);
    let off = json!({"buildScripts": false, "procMacros": false, "features": ["a"]});
    assert_answers(&call(&runtime, query(&world, off)).await, &world);
}

#[tokio::test]
async fn the_home_setting_allows_build_scripts() {
    for context in [
        json!({"buildScripts": true}),
        json!({"buildScripts": true, "procMacros": true}),
    ] {
        let world = World::new();
        let runtime = world.runtime(&[], &json!({"lsp": {"allowBuildScripts": true}}), None);
        assert_answers(&call(&runtime, query(&world, context)).await, &world);
    }
}

#[tokio::test]
async fn the_process_environment_allows_build_scripts() {
    let world = World::new();
    let env = crate::config::field::LSP_ALLOW_BUILD_SCRIPTS
        .env_name()
        .expect("bound");
    let runtime = world.runtime(&[(env, "true")], &json!({}), None);
    let row = call(&runtime, query(&world, json!({"buildScripts": true}))).await;
    assert_answers(&row, &world);
}

/// The repository being searched must not be able to allow its own build
/// scripts to run.
#[tokio::test]
async fn a_workspace_setting_cannot_allow_build_scripts() {
    let world = World::new();
    let widen = json!({"lsp": {"allowBuildScripts": true}});
    let runtime = world.runtime(&[], &json!({}), Some(&widen));
    let row = call(&runtime, query(&world, json!({"buildScripts": true}))).await;
    assert_refused(&row, &world);
}

/// The refusal's opt-in names the setting the contract declares.
#[test]
fn the_opt_in_names_the_declared_setting() {
    let allow = crate::config::field::LSP_ALLOW_BUILD_SCRIPTS;
    let opt_in = super::super::receipt::BUILD_SCRIPTS_OPT_IN;
    assert!(opt_in.contains(allow.path), "{opt_in}");
    assert!(
        opt_in.contains(allow.env_name().expect("bound")),
        "{opt_in}"
    );
}
