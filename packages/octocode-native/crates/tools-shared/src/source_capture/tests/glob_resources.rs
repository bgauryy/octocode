//! A localFetch resource whose path is a glob (`glob::expand`, C3eF R3).
use super::key::Keying;
use super::*;
use octocode_test_support::json_fixture;

fn local(path: &str) -> Value {
    json!({"tool":"localFetch","query":{"path":path}})
}

fn write(reader: &Keying, relative: &str, text: &str) {
    let path = reader.root().join(relative);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(path, text).expect("write");
}

fn context() -> ExecutionContext {
    ExecutionContext {
        cancellation: octocode_io::cancellation::Cancellation::new(),
        deadline: None,
        response_window: None,
        github_credential: None,
    }
}

fn expanded(reader: &Keying, resources: Value) -> crate::source_capture::glob::Expanded {
    expanded_under(reader, resources, &context())
}

fn expanded_under(
    reader: &Keying,
    resources: Value,
    context: &ExecutionContext,
) -> crate::source_capture::glob::Expanded {
    crate::source_capture::glob::expand(resources.as_object().expect("resources"), reader, context)
}

fn keys(out: &crate::source_capture::glob::Expanded) -> Vec<&str> {
    out.resources.iter().map(|(key, _)| key.as_str()).collect()
}

/// Every matching file is its own resource `<key>/<path under the glob's
/// directory>`, in walk order, with the rest of the query kept; a plain
/// resource stays as it is, in its place.
#[test]
fn a_glob_resource_is_one_resource_per_matching_file_in_walk_order() {
    let reader = Keying::new();
    for file in ["docs/x.md", "docs/b/y.md", "docs/b/c/w.md", "docs/z.rs"] {
        write(&reader, file, "text\n");
    }
    let mut ranged = local("docs/**/*.md");
    ranged["query"]["ranges"] = json_fixture(r#"[{"startLine": 1, "endLine": 1}]"#);
    let out = expanded(&reader, json!({"d":ranged,"plain":local("docs/z.rs")}));
    let keys = out
        .resources
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    assert_eq!(keys, ["d/b/c/w.md", "d/b/y.md", "d/x.md", "plain"]);
    let sources = out
        .resources
        .iter()
        .map(|(_, source)| source.as_ref().expect("matched").clone())
        .collect::<Vec<_>>();
    assert_eq!(sources[0]["query"]["path"], "docs/b/c/w.md");
    assert_eq!(sources[2]["query"]["path"], "docs/x.md");
    assert_eq!(sources[2]["query"]["ranges"], ranged["query"]["ranges"]);
    assert_eq!(sources[2]["tool"], "localFetch");
    assert_eq!(sources[3], local("docs/z.rs"));
    assert!(out.notices.is_empty(), "{:?}", out.notices);
}

/// A credential file the glob matches is withheld, as a localFetch read of
/// it is to a provider, and the expansion says so without naming it.
#[test]
fn a_glob_withholds_credential_files_and_discloses_them() {
    let reader = Keying::new();
    write(&reader, "keys/notes.txt", "text\n");
    write(&reader, "keys/id_rsa", "secret\n");
    let out = expanded(&reader, json!({"k":local("keys/*")}));
    let keys = out
        .resources
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    assert_eq!(keys, ["k/notes.txt"]);
    assert_eq!(out.notices.len(), 1, "{:?}", out.notices);
    assert!(
        out.notices[0].contains("credential file"),
        "{:?}",
        out.notices
    );
    assert!(!out.notices[0].contains("id_rsa"), "{:?}", out.notices);
}

/// A glob that matches nothing is that resource's error, naming the pattern;
/// a path that names an existing file is that file, glob characters and all.
#[test]
fn a_glob_matching_nothing_is_its_resources_error_and_a_literal_file_stays() {
    let reader = Keying::new();
    write(&reader, "app/[id].tsx", "export {}\n");
    let out = expanded(
        &reader,
        json!({"none":local("app/*.txt"),"page":local("app/[id].tsx")}),
    );
    let (key, none) = &out.resources[0];
    assert_eq!(key, "none");
    let error = none.as_ref().expect_err("nothing matched");
    assert_eq!(error.code, ErrorCode::NotFound);
    assert!(error.message.contains("app/*.txt"), "{}", error.message);
    assert_eq!(out.resources[1].0, "page");
    assert_eq!(
        out.resources[1].1.as_ref().expect("literal"),
        &local("app/[id].tsx")
    );
}

/// RV-C3eF F1: every file the glob names that the walk leaves out (a dot
/// file, an ignored file, a default-excluded directory, an unreadable
/// directory) is disclosed, one path per cause, as localSearch discloses its
/// `skippedPaths`.
#[test]
fn a_glob_discloses_every_path_its_walk_left_out() {
    let reader = Keying::new();
    for file in [
        "docs/a.md",
        "docs/.h.md",
        "docs/ign.md",
        "docs/node_modules/n.md",
        "docs/locked/l.md",
    ] {
        write(&reader, file, "text\n");
    }
    write(&reader, "docs/.ignore", "ign.md\n");
    let locked = reader.root().join("docs/locked");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("lock");
    }
    let unreadable = std::fs::read_dir(&locked).is_err();
    let out = expanded(&reader, json!({"d":local("docs/**/*.md")}));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("unlock");
    }
    assert_eq!(keys(&out), ["d/a.md"], "{:?}", out.notices);
    let said = out.notices.join("\n");
    for (path, cause) in [
        ("docs/.h.md", "hidden"),
        ("docs/ign.md", "ignored"),
        ("docs/node_modules", "default-excluded"),
    ] {
        assert!(
            said.contains(&format!("`{path}` ({cause}")),
            "no disclosure of {path}: {said:?}"
        );
    }
    if unreadable {
        assert!(
            said.contains("unreadable"),
            "no disclosure of docs/locked: {said:?}"
        );
    }
}

/// RV-C3eF F4: a bracket segment that names an existing directory is that
/// directory, so the glob below it matches its files.
#[test]
fn a_bracket_directory_that_exists_is_literal_in_a_glob() {
    let reader = Keying::new();
    write(&reader, "app/[id]/page.tsx", "export {}\n");
    write(&reader, "app/[id]/layout.tsx", "export {}\n");
    let out = expanded(&reader, json!({"p":local("app/[id]/*.tsx")}));
    assert_eq!(
        keys(&out),
        ["p/layout.tsx", "p/page.tsx"],
        "{:?}",
        out.resources
    );
    assert_eq!(
        out.resources[1].1.as_ref().expect("matched")["query"]["path"],
        "app/[id]/page.tsx"
    );
}

/// RV-C3eF F5: a cancelled call stops the glob's walk; its resource says
/// so, and a plain resource is untouched.
#[test]
fn a_cancelled_call_stops_the_glob_walk() {
    let reader = Keying::new();
    for index in 0..20 {
        write(&reader, &format!("many/f{index}.md"), "text\n");
    }
    let context = context();
    context.cancellation.cancel();
    let out = expanded_under(
        &reader,
        json!({"g":local("many/*.md"),"plain":local("many/f1.md")}),
        &context,
    );
    assert_eq!(keys(&out), ["g", "plain"], "{:?}", out.resources);
    let error = out.resources[0].1.as_ref().expect_err("stopped");
    assert_eq!(error.code, ErrorCode::Cancelled, "{}", error.message);
}

/// RV-C3eF R2-1: a cause that left out several paths counts them under the
/// directory holding them, and a ready structureSearch call lists every file
/// the glob matches with exactly those skips lifted.
#[test]
fn a_glob_names_each_skip_cause_and_offers_the_listing_of_all() {
    let reader = Keying::new();
    for file in [
        "docs/sub/keep.md",
        "docs/sub/parent-ignored.md",
        "docs/sub/build/b1.md",
        "docs/sub/build/b2.md",
        "docs/sub/gen/i1.md",
        "docs/sub/gen/i2.md",
        "docs/sub/.h1.md",
        "docs/sub/.h2.md",
    ] {
        write(&reader, file, "text\n");
    }
    // Two ignore rules: one above the glob's directory, one nested in it.
    write(&reader, ".ignore", "docs/sub/parent-ignored.md\nbuild/\n");
    write(&reader, "docs/sub/gen/.ignore", "*.md\n");
    let out = expanded(&reader, json!({"d":local("docs/sub/**/*.md")}));
    assert_eq!(keys(&out), ["d/keep.md"], "{:?}", out.notices);
    let said = out.notices.join("\n");
    for entry in ["2 hidden under `docs/sub`", "4 ignored under `docs/sub`"] {
        assert!(said.contains(entry), "no {entry:?} in {said:?}");
    }
    let listing = out
        .related
        .iter()
        .find(|call| call["tool"] == "structureSearch")
        .unwrap_or_else(|| panic!("a listing of every skipped file: {:?}", out.related));
    let query = &listing["input"]["queries"][0];
    assert_eq!(query["path"], "docs/sub", "{listing:#}");
    assert_eq!(
        query["include"],
        json_fixture(r#"["/**/*.md"]"#),
        "{listing:#}"
    );
    assert_eq!(query["includeIgnored"], true, "{listing:#}");
    assert_eq!(query["includeHidden"], true, "{listing:#}");
    // The ignored `build/` also bears a default-excluded name: reaching it
    // lifts both, as localSearch's lead does.
    assert_eq!(query["defaultExcludes"], false, "{listing:#}");
}

/// RV-C3eF R2-2: an expanded key never takes a key the caller gave or one
/// another glob produced; it takes the next free `-<n>`.
#[test]
fn an_expanded_key_never_collides_with_another_key() {
    let reader = Keying::new();
    write(&reader, "docs/a.md", "text\n");
    write(&reader, "docs/z.rs", "text\n");
    let out = expanded(
        &reader,
        json!({"d":local("docs/*.md"),"d/a.md":local("docs/z.rs"),"e":local("docs/*.md")}),
    );
    assert_eq!(
        keys(&out),
        ["d/a.md-2", "d/a.md", "e/a.md"],
        "{:?}",
        out.resources
    );
    assert_eq!(
        out.resources[1].1.as_ref().expect("caller's own"),
        &local("docs/z.rs")
    );
}

/// RV-C3eF R2-5: a skipped path the contract cannot carry costs only its own
/// name: every other path is still named, and the lost one is counted.
#[test]
fn a_skip_that_cannot_be_named_is_counted_and_the_rest_named() {
    use crate::leads::skipped::SkippedEntry;
    use octocode_contract_types::SkippedPathCause;
    let root = std::path::Path::new("/w/docs");
    let said = crate::source_capture::glob::disclose_skips(
        "docs/*.md",
        root,
        "docs",
        vec![
            SkippedEntry::path("/w/docs/.h.md", SkippedPathCause::Hidden),
            SkippedEntry::path("", SkippedPathCause::Ignored),
        ],
        "1 ignored, 1 hidden",
    )
    .expect("a disclosure");
    assert!(said.contains("`docs/.h.md` (hidden)"), "{said}");
    assert!(said.contains("1 more that cannot be named"), "{said}");
}

/// A glob over a folder holding a renamed kubeconfig withholds it unnamed and
/// keeps the Go source beside it (RV-C3eF live: `cred2/**/*` sent a renamed
/// kubeconfig to the provider; round 12).
#[test]
fn a_glob_withholds_a_renamed_kubeconfig_and_keeps_its_source() {
    let reader = Keying::new();
    write(
        &reader,
        "cred2/kube/prod.kubeconfig",
        "apiVersion: v1\nkind: Config\n",
    );
    write(&reader, "cred2/kube/kubeconfig.go", "package kube\n");
    let out = expanded(&reader, json!({"k":local("cred2/**/*")}));
    let keys = out
        .resources
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    assert_eq!(keys, ["k/kube/kubeconfig.go"], "{:?}", out.notices);
    assert_eq!(out.notices.len(), 1, "{:?}", out.notices);
    assert!(
        out.notices[0].contains("credential file"),
        "{:?}",
        out.notices
    );
    assert!(
        !out.notices[0].contains("prod.kubeconfig"),
        "never named: {:?}",
        out.notices
    );
}

/// Live check D2: withheld files are counted under the directory that holds
/// them, never worded as if that directory were one of them.
#[test]
fn withheld_files_are_counted_under_their_directory() {
    let reader = Keying::new();
    write(&reader, "cred2/id_rsa", "secret\n");
    write(&reader, "cred2/credentials", "secret\n");
    write(&reader, "cred2/notes.txt", "text\n");
    let out = expanded(&reader, json!({"c":local("cred2/*")}));
    assert_eq!(keys(&out), ["c/notes.txt"], "{:?}", out.notices);
    let said = out.notices.join("\n");
    assert!(
        said.contains("2 withheld under `cred2` (credential file)"),
        "{said:?}"
    );
    assert!(!said.contains("and 1 more"), "{said:?}");
}

/// Live check D3: a glob whose directory does not exist inside the allowed
/// roots says that directory does not exist; only a directory outside them
/// gets the allowed-roots hint.
#[test]
fn a_glob_under_a_missing_directory_says_the_directory_does_not_exist() {
    let reader = Keying::new();
    let out = expanded(
        &reader,
        json!({"x":local("nothing/*.md"),"o":local("/definitely-outside-roots/*.md")}),
    );
    let missing = out.resources[0].1.as_ref().expect_err("no directory");
    assert_eq!(missing.code, ErrorCode::NotFound, "{}", missing.message);
    assert!(
        missing
            .hints
            .iter()
            .any(|hint| hint.contains("`nothing` does not exist")),
        "{:?}",
        missing.hints
    );
    assert!(
        !missing
            .hints
            .iter()
            .any(|hint| hint.contains("allowed roots")),
        "{:?}",
        missing.hints
    );
    let outside = out.resources[1].1.as_ref().expect_err("outside the roots");
    assert_eq!(
        outside.code,
        ErrorCode::OutsideAllowedRoots,
        "{}",
        outside.message
    );
    assert!(
        outside
            .hints
            .iter()
            .any(|hint| hint.contains("allowed roots")),
        "{:?}",
        outside.hints
    );
}
