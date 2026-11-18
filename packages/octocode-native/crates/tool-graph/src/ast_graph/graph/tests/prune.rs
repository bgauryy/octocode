//! The graph scans what every local tool walks: one default prune, so the
//! graph, its staleness scan and the LSP fingerprint (`tools::pruned_walk`)
//! cover the same files.

use super::fixture_graph;
use std::collections::BTreeSet;

#[test]
fn graph_discloses_withheld_subtrees_without_guessing_their_file_count() {
    let root = octocode_runtime_base::test_support::Fixture::new();
    root.write("src/main.ts", "export const visible = 1;\n");
    root.write("src/.aws/not-disclosed.ts", "export const hidden = 1;\n");
    let built = fixture_graph(&root.0, &[]);
    assert_eq!(built.facts.len(), 1);
    let diagnostic = built
        .diagnostics
        .iter()
        .find(|diag| diag.code.as_str() == "path-policy-withheld")
        .expect("the withheld subtree is a disclosed coverage gap");
    assert_eq!(diagnostic.file, ".");
    assert!(diagnostic.message.contains(".aws/"), "{diagnostic:?}");
    assert!(diagnostic.message.contains("unproven"), "{diagnostic:?}");
    assert!(!diagnostic.message.contains("not-disclosed.ts"));
    assert_eq!(
        built.files_skipped, 0,
        "the withheld directory's file count is unknown"
    );
}

#[test]
fn credential_named_source_directories_are_scanned_and_linked() {
    for name in ["credentials", "source.pem", "source.key", "id_rsa.old"] {
        let root = octocode_runtime_base::test_support::Fixture::new();
        let source = format!("src/{name}/hasher.ts");
        root.write(&source, "export function hashPassword() { return 42; }\n");
        root.write("src/main.ts", format!("import {{ hashPassword }} from './{name}/hasher';\nexport const hash = hashPassword();\n"));
        let built = fixture_graph(&root.0, &[]);
        assert!(
            built.facts.contains_key(&source),
            "{name}: {:?}",
            built.facts.keys()
        );
        assert_eq!(built.facts.len(), 2, "{name}");
        assert_eq!(
            built.imports[super::super::super::types::ImportOutcome::Resolved as usize],
            1,
            "{name}: {:?}",
            built.diagnostics
        );
        assert!(
            built.diagnostics.is_empty(),
            "{name}: {:?}",
            built.diagnostics
        );
    }
}

/// A tree with source in `src/`, `build/` and `node_modules/`; with `git`, a
/// work tree that ignores `node_modules/` only (so `build/` is git's own).
fn tree(git: bool) -> octocode_runtime_base::test_support::Fixture {
    let root = octocode_runtime_base::test_support::Fixture::new();
    root.write("src/app.ts", "export const app = 1;\n");
    root.write("build/gen.ts", "export const gen = 1;\n");
    root.write("node_modules/dep/index.ts", "export const dep = 1;\n");
    if git {
        std::fs::create_dir(root.0.join(".git")).expect("repository marker");
        root.write(".gitignore", "node_modules/\n");
    }
    root
}

/// The `.ts` files a tool's own pruned walk of `root` reaches.
fn own_walk(root: &std::path::Path) -> BTreeSet<String> {
    octocode_tools_shared::pruned_walk(root, [], Default::default())
        .build()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "ts"))
        .filter_map(|entry| {
            let relative = entry.path().strip_prefix(root).ok()?;
            Some(octocode_engine::text::slash_path(relative))
        })
        .collect()
}

/// Lifting every walk filter (`includeHidden`, `includeIgnored`,
/// `defaultExcludes:false`) admits dot, ignored and default-pruned sources,
/// never the credential stores: the path policy withholds `.ssh/`, `.aws/`,
/// `.env` and `.env.*` files whatever the walk, and says so.
#[test]
fn a_lifted_walk_still_withholds_credential_stores() {
    use crate::ast_graph::walk_scope::WalkScope;
    use octocode_engine::types::IgnorePolicy;
    let root = octocode_runtime_base::test_support::Fixture::new();
    for (path, source) in [
        ("src/a.py", "def a():\n    pass\n"),
        (".hid/h.py", "def h():\n    pass\n"),
        ("ign.py", "def ign():\n    pass\n"),
        ("node_modules/p/n.py", "def n():\n    pass\n"),
        (".ssh/k.py", "def k():\n    pass\n"),
        (".aws/c.py", "def c():\n    pass\n"),
        (".env.py", "def env():\n    pass\n"),
        (".env", "TOKEN=1\n"),
        (".ignore", "ign.py\n"),
    ] {
        root.write(path, source);
    }
    let paths = octocode_runtime_base::test_support::workspace_policy(&root.0);
    let built = |walk| {
        super::super::build_graph(
            &super::super::GraphScan {
                root: root.0.to_path_buf(),
                walk,
                ..Default::default()
            },
            &paths,
            &octocode_runtime_base::security::ContentSecurity::new(),
            &octocode_runtime_base::execution::cancel::NeverCancel,
        )
        .expect("graph")
    };
    let files =
        |graph: &super::super::BuiltGraph| graph.facts.keys().cloned().collect::<BTreeSet<_>>();
    let owned = |paths: &[&str]| {
        paths
            .iter()
            .map(|p| (*p).to_owned())
            .collect::<BTreeSet<_>>()
    };
    let plain = built(WalkScope::default());
    assert_eq!(files(&plain), owned(&["src/a.py"]));
    let lifted = built(WalkScope {
        default_excludes: false,
        ignore: IgnorePolicy {
            include_ignored: true,
            include_hidden: true,
        },
    });
    assert_eq!(
        files(&lifted),
        owned(&[".hid/h.py", "ign.py", "node_modules/p/n.py", "src/a.py"]),
        "{:?}",
        lifted.diagnostics
    );
    let notice = lifted
        .diagnostics
        .iter()
        .find(|diag| diag.code.as_str() == "path-policy-withheld")
        .expect("the withheld stores are a disclosed coverage gap");
    for store in [".ssh/", ".aws/"] {
        assert!(notice.message.contains(store), "{notice:?}");
    }
}

#[test]
fn a_git_tracked_build_dir_is_scanned_as_every_tool_walks_it() {
    for git in [true, false] {
        let root = tree(git);
        let graph = fixture_graph(&root.0, &[])
            .facts
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let expected = if git {
            ["build/gen.ts", "src/app.ts"].as_slice()
        } else {
            ["src/app.ts"].as_slice()
        };
        let expected = expected
            .iter()
            .map(|path| (*path).to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(graph, expected, "git work tree: {git}");
        assert_eq!(own_walk(&root.0), expected, "git work tree: {git}");
    }
}

/// What the one prune leaves out is a named gap, one diagnostic per
/// directory, and a directory only this graph used to skip is scanned.
#[test]
fn a_default_pruned_directory_is_disclosed_and_obj_is_scanned() {
    let root = octocode_runtime_base::test_support::Fixture::new();
    root.write("src/app.ts", "export const app = 1;\n");
    root.write("obj/gen.ts", "export const gen = 1;\n");
    root.write("src/node_modules/dep/index.ts", "export const dep = 1;\n");
    root.write("dist/out.ts", "export const out = 1;\n");
    let built = fixture_graph(&root.0, &[]);
    assert_eq!(
        built.facts.keys().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from(["obj/gen.ts".to_owned(), "src/app.ts".to_owned()]),
    );
    let disclosed = built
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_str() == "default-excluded")
        .map(|diagnostic| diagnostic.file.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        disclosed,
        ["dist", "src/node_modules"],
        "{:?}",
        built.diagnostics
    );
    // A caller's own exclusion is no default prune.
    let narrowed = fixture_graph(&root.0, &["obj"]);
    assert!(!narrowed.facts.contains_key("obj/gen.ts"));
    assert!(
        !narrowed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.file == "obj")
    );
}
