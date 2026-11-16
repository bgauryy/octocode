use super::fixture::FakeToolchains;
use super::launch_identity;
use crate::lsp::types::LspLanguageServerConfig;
use std::collections::HashMap;

fn config(
    workspace: &std::path::Path,
    language: &str,
    command: &str,
    env: HashMap<String, String>,
) -> LspLanguageServerConfig {
    LspLanguageServerConfig {
        command: command.to_owned(),
        args: None,
        workspace_root: workspace.to_string_lossy().into_owned(),
        language_id: Some(language.to_owned()),
        document_language_ids: Default::default(),
        initialization_options: None,
        env: Some(env),
        max_memory_mb: None,
        source: Default::default(),
    }
}

fn rust(tc: &FakeToolchains, env: HashMap<String, String>) -> LspLanguageServerConfig {
    let workspace = tc.root.join("work");
    std::fs::create_dir_all(&workspace).expect("workspace");
    config(&workspace, "rust", "rust-analyzer", env)
}

/// Two clients reach the toolchain by other strings: another `PATH` order and
/// extras, a locale, a scratch directory, the toolchain named in the
/// environment or left to rustup's default. One launch.
#[test]
fn environments_resolving_to_one_toolchain_have_one_identity() {
    let tc = FakeToolchains::install();
    let gui = rust(
        &tc,
        tc.env(
            "{bin}:/usr/bin:/bin",
            &[("LANG", "C"), ("TMPDIR", "/tmp/a")],
        ),
    );
    let terminal = rust(
        &tc,
        tc.env(
            "/usr/local/bin:/usr/bin:{bin}:/bin",
            &[
                ("LANG", "en_US.UTF-8"),
                ("LC_ALL", "en_US.UTF-8"),
                ("TMPDIR", "/var/folders/b"),
                ("TEMP", "/var/folders/b"),
                ("RUSTUP_TOOLCHAIN", "stable"),
            ],
        ),
    );
    assert_eq!(launch_identity(&gui), launch_identity(&terminal));
}

/// A toolchain another environment selects is another launch, by the
/// variable or by what the proxy on `PATH` would run.
#[test]
fn environments_resolving_to_other_toolchains_have_other_identities() {
    let tc = FakeToolchains::install();
    let stable = rust(&tc, tc.env("{bin}:/usr/bin", &[]));
    let nightly = rust(
        &tc,
        tc.env("{bin}:/usr/bin", &[("RUSTUP_TOOLCHAIN", "nightly")]),
    );
    assert_ne!(launch_identity(&stable), launch_identity(&nightly));
    let cargo_home = rust(
        &tc,
        tc.env("{bin}:/usr/bin", &[("CARGO_HOME", "/elsewhere/.cargo")]),
    );
    assert_ne!(
        launch_identity(&stable),
        launch_identity(&cargo_home),
        "CARGO_HOME selects the registry and installed tools"
    );
}

/// An unset `CARGO_HOME` and one that names its default are one launch.
#[test]
fn cargo_home_is_compared_by_its_default() {
    let tc = FakeToolchains::install();
    let home = tc.root.join("home");
    std::fs::create_dir_all(home.join(".cargo")).expect("cargo home");
    let unset = rust(&tc, tc.env("{bin}:/usr/bin", &[]));
    let named = rust(
        &tc,
        tc.env(
            "{bin}:/usr/bin",
            &[("CARGO_HOME", &home.join(".cargo").to_string_lossy())],
        ),
    );
    assert_eq!(launch_identity(&unset), launch_identity(&named));
}

/// A server of no known profile keeps its whole environment: nothing says
/// which strings do not matter to it.
#[test]
fn an_unknown_language_keeps_its_environment_as_given() {
    let tc = FakeToolchains::install();
    let workspace = tc.root.join("work");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let first = config(
        &workspace,
        "plaintext",
        "server",
        tc.env("{bin}", &[("LANG", "C")]),
    );
    let second = config(
        &workspace,
        "plaintext",
        "server",
        tc.env("{bin}", &[("LANG", "en")]),
    );
    assert_ne!(launch_identity(&first), launch_identity(&second));
}

/// The JVM's default charset follows the locale: Java servers keep it.
#[test]
fn a_java_server_keeps_its_locale() {
    let tc = FakeToolchains::install();
    let workspace = tc.root.join("work");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let c = config(
        &workspace,
        "java",
        "jdtls",
        tc.env("{bin}", &[("LANG", "C")]),
    );
    let utf8 = config(
        &workspace,
        "java",
        "jdtls",
        tc.env("{bin}", &[("LANG", "en_US.UTF-8")]),
    );
    assert_ne!(launch_identity(&c), launch_identity(&utf8));
}
