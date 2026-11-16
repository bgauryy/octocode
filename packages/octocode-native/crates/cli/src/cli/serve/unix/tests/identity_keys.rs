use super::*;

/// The process identity of a CLI started with `env` (and `OCTOCODE_HOME` =
/// `home`), as it sends it.
fn process_of(home: &Path, env: &[(&str, &str)]) -> String {
    let mut vars: BTreeMap<String, String> = env
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    vars.entry("OCTOCODE_HOME".into())
        .or_insert_with(|| home.to_string_lossy().into_owned());
    process_identity(&served_env(vars))
}

/// A `cd` changes the shell's directory variables only: the call keeps
/// its runtime. Any other variable is another runtime.
#[test]
fn a_cd_keeps_the_runtime_of_a_directory() {
    let runtime = |env: &[(&str, &str)]| {
        let env = env
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        env_digest(&served_env(env))
    };
    let base = runtime(&[("GH_CONFIG_DIR", "/a"), ("PWD", "/w"), ("OLDPWD", "/x")]);
    assert_eq!(
        runtime(&[("GH_CONFIG_DIR", "/a"), ("PWD", "/w/sub"), ("OLDPWD", "/w")]),
        base
    );
    assert_eq!(runtime(&[("GH_CONFIG_DIR", "/a")]), base);
    assert_ne!(runtime(&[("GH_CONFIG_DIR", "/b"), ("PWD", "/w")]), base);
}

/// A server is keyed by the build, the workspace and the environment the
/// engine reads from the server's own process; nothing else. The shell's
/// `PWD`, a variable nothing reads, a token and every setting leave it
/// alone, and two directories of one repository share a server.
#[test]
fn the_server_key_covers_the_build_the_workspace_and_the_process_environment() {
    let home = tempfile::tempdir().expect("home");
    let repo = tempfile::tempdir().expect("repo");
    std::fs::create_dir_all(repo.path().join(".git")).expect("git");
    let (a, b) = (repo.path().join("a"), repo.path().join("b"));
    std::fs::create_dir_all(&a).expect("a");
    std::fs::create_dir_all(&b).expect("b");
    let key = |cwd: &Path, env: &[(&str, &str)]| {
        server_key("exe|1|2", &workspace_of(cwd), &process_of(home.path(), env))
    };
    let base = key(&a, &[("GITHUB_TOKEN", "t1")]);
    assert_eq!(base.len(), 16);
    assert!(!base.contains("t1"));
    assert_eq!(key(&a, &[("GITHUB_TOKEN", "t1")]), base);
    // One server for two directories of one repository.
    assert_eq!(key(&b, &[("GITHUB_TOKEN", "t1")]), base);
    assert_eq!(workspace_of(&a), repo.path());
    let other_home = tempfile::tempdir().expect("other home");
    let other_home = other_home.path().to_string_lossy().into_owned();
    for (what, env) in [
        ("PWD", vec![("GITHUB_TOKEN", "t1"), ("PWD", "/elsewhere")]),
        (
            "an unrelated variable",
            vec![("GITHUB_TOKEN", "t1"), ("TERM_SESSION_ID", "x")],
        ),
        (
            "an unread OCTOCODE_ variable",
            vec![("GITHUB_TOKEN", "t1"), ("OCTOCODE_X", "1")],
        ),
        // Configuration keys the runtime, not the server.
        ("token", vec![("GITHUB_TOKEN", "t2")]),
        ("token source", vec![("GH_TOKEN", "t1")]),
        ("no token", vec![]),
        (
            "OCTOCODE_BETA",
            vec![("GITHUB_TOKEN", "t1"), ("OCTOCODE_BETA", "true")],
        ),
        (
            "OCTOCODE_HOME",
            vec![("GITHUB_TOKEN", "t1"), ("OCTOCODE_HOME", &other_home)],
        ),
    ] {
        assert_eq!(key(&a, &env), base, "{what}");
    }
    // Every variable the engine reads from the server's own process (the
    // walker's global ignore rules, `HOME`) and every proxy and trust-store
    // variable: a call cannot bring its own value, so another value is
    // another server.
    for name in octocode_engine::process_env::PROCESS_ENV
        .iter()
        .chain(octocode_io::http::NETWORK_ENV)
    {
        assert_ne!(
            key(&a, &[("GITHUB_TOKEN", "t1"), (name, "/elsewhere")]),
            base,
            "{name}"
        );
    }
    // A variable a call carries to language-server discovery and start
    // (`PATH`, `LANG`, the toolchain variables) keys the runtime, not the
    // server: a GUI-launched MCP server and a terminal's CLI share one.
    for name in octocode_engine::process_env::CALL_ENV
        .iter()
        .filter(|name| !octocode_engine::process_env::declares(name))
    {
        assert_eq!(
            key(&a, &[("GITHUB_TOKEN", "t1"), (name, "/elsewhere")]),
            base,
            "{name}"
        );
    }
    // Another repository, another build.
    let env = [("GITHUB_TOKEN", "t1")];
    let elsewhere = tempfile::tempdir().expect("elsewhere");
    assert_ne!(key(elsewhere.path(), &env), key(&b, &env), "workspace");
    let process = process_of(home.path(), &env);
    assert_ne!(
        server_key("exe|1|3", &workspace_of(&b), &process),
        server_key("exe|1|2", &workspace_of(&b), &process),
        "build"
    );
}

/// The configuration (token, `OCTOCODE_*` settings, the home `.env`, the
/// home and workspace `.octocoderc` files) is the runtime's identity: each
/// changes it, and none changes the server's key.
#[test]
fn configuration_keys_the_runtime_and_not_the_server() {
    let home = tempfile::tempdir().expect("home");
    let repo = tempfile::tempdir().expect("repo");
    std::fs::create_dir_all(repo.path().join(".git")).expect("git");
    let cwd = repo.path().join("a");
    std::fs::create_dir_all(&cwd).expect("cwd");
    let env = [("GITHUB_TOKEN", "t1")];
    let key = || {
        server_key(
            "exe|1|2",
            &workspace_of(&cwd),
            &process_of(home.path(), &env),
        )
    };
    let (server, runtime) = (key(), identity(home.path(), &cwd, &env).1);
    let other_home = tempfile::tempdir().expect("other home");
    let other_home = other_home.path().to_string_lossy().into_owned();
    for (what, vars) in [
        ("token", vec![("GITHUB_TOKEN", "t2")]),
        ("token source", vec![("GH_TOKEN", "t1")]),
        ("no token", vec![]),
        (
            "OCTOCODE_BETA",
            vec![("GITHUB_TOKEN", "t1"), ("OCTOCODE_BETA", "true")],
        ),
        (
            "OCTOCODE_HOME",
            vec![("GITHUB_TOKEN", "t1"), ("OCTOCODE_HOME", &other_home)],
        ),
    ] {
        assert_ne!(identity(home.path(), &cwd, &vars).1, runtime, "{what}");
    }
    let mut last = runtime;
    for (what, path, text) in [
        (
            "home .env",
            home.path().join(".env"),
            "OCTOCODE_REDACT_EMAILS=true\n",
        ),
        (
            "home .octocoderc",
            home.path().join(".octocoderc"),
            "{\"local\":{\"enabled\":false}}",
        ),
        (
            "workspace .octocoderc",
            cwd.join(".octocode/.octocoderc"),
            "{\"lsp\":{\"prewarm\":\"off\"}}",
        ),
    ] {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(&path, text).expect(what);
        let now = identity(home.path(), &cwd, &env).1;
        assert_ne!(now, last, "{what}");
        last = now;
        assert_eq!(key(), server, "{what} leaves the server key");
    }
}

/// Another version of the program is another build, whatever its file.
#[test]
fn different_versions_do_not_share_a_server() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("octocode");
    std::fs::write(&file, b"one build").expect("binary");
    let meta = std::fs::metadata(&file).expect("meta");
    assert_ne!(
        file_identity(&meta, "20.2.0"),
        file_identity(&meta, "20.2.1")
    );
    assert_eq!(
        file_identity(&meta, "20.2.0"),
        file_identity(&meta, "20.2.0")
    );
}

/// A symlink to the binary, and a spelling of its path through `..`, key
/// the same build as its real path: they reach one server. A rebuild (new
/// contents) is another build.
#[test]
fn every_path_to_one_binary_is_one_build() {
    let dir = tempfile::tempdir().expect("dir");
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    let real = bin.join("octocode");
    std::fs::write(&real, b"build one").expect("binary");
    let link = dir.path().join("octocode-link");
    std::os::unix::fs::symlink(&real, &link).expect("symlink");
    let identity = build_identity(&real).expect("real path");
    assert_eq!(build_identity(&link), Some(identity.clone()));
    assert_eq!(
        build_identity(&bin.join("..").join("bin").join("octocode")),
        Some(identity.clone())
    );
    std::fs::write(&real, b"build two, rebuilt").expect("rebuild");
    assert_ne!(build_identity(&link), Some(identity));
}

/// A hard link is the same build: one file, one server. A copy is another
/// file with its own server, though its bytes are equal.
#[test]
fn a_hard_link_is_one_build_and_a_copy_is_another() {
    let dir = tempfile::tempdir().expect("dir");
    let real = dir.path().join("octocode");
    std::fs::write(&real, b"one build").expect("binary");
    let linked = dir.path().join("octocode-linked");
    std::fs::hard_link(&real, &linked).expect("hard link");
    let copied = dir.path().join("octocode-copied");
    std::fs::copy(&real, &copied).expect("copy");
    let identity = build_identity(&real).expect("real");
    assert_eq!(build_identity(&linked), Some(identity.clone()));
    assert_ne!(build_identity(&copied), Some(identity));
}
