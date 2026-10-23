use super::*;

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

/// Every input that changes what a call does changes the server key;
/// the shell's `PWD` and unrelated variables do not, and two
/// directories of one repository share a server.
#[test]
fn the_server_key_covers_every_identity_input() {
    let home = tempfile::tempdir().expect("home");
    let repo = tempfile::tempdir().expect("repo");
    std::fs::create_dir_all(repo.path().join(".git")).expect("git");
    let (a, b) = (repo.path().join("a"), repo.path().join("b"));
    std::fs::create_dir_all(&a).expect("a");
    std::fs::create_dir_all(&b).expect("b");
    let key = |cwd: &Path, env: &[(&str, &str)]| {
        server_key(
            "exe|1|2",
            &workspace_of(cwd),
            &identity(home.path(), cwd, env).1,
        )
    };
    let base = key(&a, &[("GITHUB_TOKEN", "t1")]);
    assert_eq!(base.len(), 16);
    assert!(!base.contains("t1"));
    assert_eq!(key(&a, &[("GITHUB_TOKEN", "t1")]), base);
    // One server for two directories of one repository.
    assert_eq!(key(&b, &[("GITHUB_TOKEN", "t1")]), base);
    assert_eq!(workspace_of(&a), repo.path());
    for (what, env) in [
        ("PWD", vec![("GITHUB_TOKEN", "t1"), ("PWD", "/elsewhere")]),
        (
            "an unrelated variable",
            vec![("GITHUB_TOKEN", "t1"), ("TERM_SESSION_ID", "x")],
        ),
    ] {
        assert_eq!(key(&a, &env), base, "{what}");
    }
    let other_home = tempfile::tempdir().expect("other home");
    let other_home = other_home.path().to_string_lossy().into_owned();
    for (what, env) in [
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
        (
            "an undeclared OCTOCODE_ variable",
            vec![("GITHUB_TOKEN", "t1"), ("OCTOCODE_X", "1")],
        ),
        (
            "proxy",
            vec![("GITHUB_TOKEN", "t1"), ("HTTPS_PROXY", "http://proxy:3128")],
        ),
        (
            "trust store",
            vec![("GITHUB_TOKEN", "t1"), ("SSL_CERT_FILE", "/ca.pem")],
        ),
        ("PATH", vec![("GITHUB_TOKEN", "t1"), ("PATH", "/opt/bin")]),
    ] {
        assert_ne!(key(&a, &env), base, "{what}");
    }
    // Every variable the engine reads from the server's own process: a
    // call cannot bring its own value, so another value is another server.
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
    // The home `.env`, the home and workspace `.octocoderc` files.
    let env = [("GITHUB_TOKEN", "t1")];
    std::fs::write(home.path().join(".env"), "OCTOCODE_REDACT_EMAILS=true\n").expect(".env");
    let with_env = key(&a, &env);
    assert_ne!(with_env, base, "home .env");
    std::fs::write(
        home.path().join(".octocoderc"),
        "{\"local\":{\"enabled\":false}}",
    )
    .expect("rc");
    let with_rc = key(&a, &env);
    assert_ne!(with_rc, with_env, "home .octocoderc");
    std::fs::create_dir_all(a.join(".octocode")).expect("ws");
    std::fs::write(
        a.join(".octocode/.octocoderc"),
        "{\"lsp\":{\"prewarm\":\"off\"}}",
    )
    .expect("ws rc");
    assert_ne!(key(&a, &env), with_rc, "workspace .octocoderc");
    // Another repository, another build.
    let elsewhere = tempfile::tempdir().expect("elsewhere");
    assert_ne!(key(elsewhere.path(), &env), key(&b, &env), "workspace");
    let identity = identity(home.path(), &b, &env).1;
    assert_ne!(
        server_key("exe|1|3", &workspace_of(&b), &identity),
        server_key("exe|1|2", &workspace_of(&b), &identity),
        "build"
    );
}
