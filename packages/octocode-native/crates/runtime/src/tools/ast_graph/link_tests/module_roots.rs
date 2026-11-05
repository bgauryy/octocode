//! Where each language's imports start from, whatever directory the project
//! sits in, and what an import that links nowhere is called: `external-package`
//! only when nothing in the repository could be what it names.
use super::*;

/// The `imports` tally of the snapshot at `scope`, one count per outcome.
pub(super) fn tally(root: &Path, scope: &str) -> (u64, u64, u64) {
    let imports = ask_with(root, "stats", None, |o| o.path = Some(scope.into()))["imports"].clone();
    let count = |key: &str| imports[key].as_u64().unwrap_or(0);
    (
        count("resolved"),
        count("external"),
        count("unresolvedInternal"),
    )
}

/// The `(callee, reason)` rows `unresolved` reports for the snapshot at `scope`.
pub(super) fn reasons(root: &Path, scope: &str) -> Vec<(String, String)> {
    let out = ask_with(root, "unresolved", None, |o| {
        o.path = Some(scope.into());
        o.edges = vec!["calls".into()];
    });
    let mut rows = out["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or_default().to_owned();
            (text("targetName"), text("reason"))
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

pub(super) fn row(name: &str, reason: &str) -> (String, String) {
    (name.to_owned(), reason.to_owned())
}

const TSCONFIG: &str =
    r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"],"~shared/*":["../shared/*"]}}}"#;

/// A `baseUrl` above the scan root still anchors specifiers that land inside
/// it; one landing outside, or a `paths` target outside, is unscanned code.
#[test]
fn typescript_base_url_and_paths_above_the_scan_root() {
    let temp = fixture(&[
        (".git/HEAD", "ref: refs/heads/main\n"),
        ("web/tsconfig.json", TSCONFIG),
        ("web/package.json", r#"{"name":"web"}"#),
        ("web/other/x.ts", "export function x() { return 1; }\n"),
        ("shared/y.ts", "export function y() { return 1; }\n"),
        ("web/src/lib/a.ts", "export function f() { return 1; }\n"),
        (
            "web/src/use.ts",
            "import { f } from \"src/lib/a\";\nimport { f as g } from \"@/lib/a\";\nimport { x } from \"other/x\";\nimport { y } from \"~shared/y\";\nimport { z } from \"zod\";\nexport function run() { f(); g(); x(); y(); z(); }\n",
        ),
    ]);
    let root = temp.0.as_path();
    assert_eq!(ingest(root, &root.join("web/src")).value.get("error"), None);
    assert_eq!(tally(root, "web/src"), (2, 1, 2));
    let out = reasons(root, "web/src");
    assert!(out.contains(&row("x", "other")), "{out:?}");
    assert!(out.contains(&row("y", "other")), "{out:?}");
    assert!(out.contains(&row("z", "external-package")), "{out:?}");
}

/// A bare path a scanned file ends in is no proven package.
#[test]
fn typescript_bare_path_matching_a_scanned_file_is_not_external() {
    let temp = fixture(&[
        ("web/lib/a.ts", "export function f() { return 1; }\n"),
        (
            "web/use.ts",
            "import { f } from \"lib/a\";\nexport function run() { f(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(tally(root, "."), (0, 0, 1));
    assert_eq!(reasons(root, "."), [row("f", "other")]);
}

/// A sibling of an enclosing workspace, and a `workspace:` dependency, are this
/// repository's packages though the scan holds neither.
#[test]
fn typescript_workspace_sibling_outside_the_scan_is_not_external() {
    let temp = fixture(&[
        (".git/HEAD", "ref: refs/heads/main\n"),
        (
            "package.json",
            r#"{"name":"root","workspaces":["packages/*"]}"#,
        ),
        ("packages/lib/package.json", r#"{"name":"@acme/lib"}"#),
        (
            "packages/lib/index.ts",
            "export function f() { return 1; }\n",
        ),
        (
            "packages/app/package.json",
            r#"{"name":"app","dependencies":{"linked":"file:../../linked","react":"^19"}}"#,
        ),
        (
            "packages/app/use.ts",
            "import { f } from \"@acme/lib\";\nimport { l } from \"linked\";\nimport { r } from \"react\";\nexport function run() { f(); l(); r(); }\n",
        ),
    ]);
    let root = temp.0.as_path();
    assert_eq!(
        ingest(root, &root.join("packages/app")).value.get("error"),
        None
    );
    assert_eq!(tally(root, "packages/app"), (0, 1, 2));
    assert_eq!(
        reasons(root, "packages/app"),
        [
            row("f", "other"),
            row("l", "other"),
            row("r", "external-package")
        ]
    );
}

/// A package of the scan's module, a local `replace` and a `go.work` member are
/// this repository's code though the scan holds none of them.
#[test]
fn go_local_modules_outside_the_scan_are_not_external() {
    let temp = fixture(&[
        (".git/HEAD", "ref: refs/heads/main\n"),
        (
            "svc/go.mod",
            "module x.io/m\n\nreplace x.io/rep => ../rep\n",
        ),
        ("go.work", "go 1.22\n\nuse (\n\t./svc\n\t./mem\n)\n"),
        ("mem/go.mod", "module x.io/mem\n"),
        ("svc/b/b.go", "package b\n\nfunc B() {}\n"),
        (
            "svc/a/a.go",
            "package a\n\nimport (\n\t\"fmt\"\n\t\"x.io/m/b\"\n\t\"x.io/mem/u\"\n\t\"x.io/rep/v\"\n)\n\nfunc A() {\n\tb.B()\n\tu.U()\n\tv.V()\n\tfmt.Println()\n}\n",
        ),
    ]);
    let root = temp.0.as_path();
    assert_eq!(ingest(root, &root.join("svc/a")).value.get("error"), None);
    assert_eq!(tally(root, "svc/a"), (0, 1, 3));
    let out = reasons(root, "svc/a");
    assert!(out.contains(&row("b.B", "other")), "{out:?}");
    assert!(
        out.contains(&row("fmt.Println", "external-package")),
        "{out:?}"
    );
}

/// A scan below a Java source root links what its package path reaches inside
/// the scan and calls a class above the scan unscanned; a class the scan's
/// package lacks may share another file. Only a package nothing holds is external.
#[test]
fn java_scan_below_its_source_root() {
    let temp = fixture(&[
        (".git/HEAD", "ref: refs/heads/main\n"),
        (
            "src/com/acme/B.java",
            "package com.acme;\npublic class B { public static void go() {} }\n",
        ),
        (
            "src/com/acme/app/Near.java",
            "package com.acme.app;\npublic class Near { public static void go() {} }\n",
        ),
        (
            "src/com/acme/app/A.java",
            "package com.acme.app;\nimport com.acme.B;\nimport com.acme.app.Near;\nimport com.acme.app.Gone;\nimport org.lib.Far;\nclass A { void run() { B.go(); Near.go(); Gone.go(); Far.go(); } }\n",
        ),
    ]);
    let root = temp.0.as_path();
    assert_eq!(
        ingest(root, &root.join("src/com/acme/app"))
            .value
            .get("error"),
        None
    );
    assert_eq!(tally(root, "src/com/acme/app"), (1, 1, 2));
    let out = reasons(root, "src/com/acme/app");
    assert!(out.contains(&row("B.go", "other")), "{out:?}");
    assert!(out.contains(&row("Far.go", "external-package")), "{out:?}");
}

/// A path dependency is local code wherever it lies; a crate path with no module
/// segment calls the crate's own root module.
#[test]
fn rust_crate_paths_from_a_subdirectory_and_path_dependencies() {
    let temp = fixture(&[
        (
            "crates/lib/Cargo.toml",
            "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("crates/lib/src/lib.rs", "pub fn f() {}\n"),
        (
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nmylib = { path = \"../lib\" }\n",
        ),
        (
            "crates/app/src/main.rs",
            "use mylib::f;\nfn main() {\n    f();\n    mylib::f();\n}\n",
        ),
    ]);
    let root = temp.0.as_path();
    assert_eq!(ingest(root, root).value.get("error"), None);
    assert_eq!(reasons(root, "."), []);
    assert_eq!(
        ingest(root, &root.join("crates/app")).value.get("error"),
        None
    );
    assert_eq!(tally(root, "crates/app"), (0, 0, 1));
}

/// With no manifest to say, an import of another crate or module path is an
/// unknown; only the standard library's is known.
#[test]
fn a_file_no_manifest_encloses_claims_only_the_standard_library() {
    let temp = fixture(&[
        (".git/HEAD", "ref: refs/heads/main\n"),
        (
            "loose.rs",
            "use std::fmt;\nuse serde::Deserialize;\nfn f() {}\n",
        ),
        (
            "main.go",
            "package main\n\nimport (\n\t\"fmt\"\n\t\"github.com/acme/lib\"\n)\n\nfunc main() {\n\tfmt.Println()\n\tlib.Do()\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(tally(root, "."), (0, 2, 2));
}

/// A quoted include the search path missed is the project's gap; an angle
/// include stays a system header.
#[test]
fn c_quoted_include_that_misses_is_not_external() {
    let temp = fixture(&[(
        "src/a.c",
        "#include \"missing.h\"\n#include <stdio.h>\nint f(void) { return 1; }\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(tally(root, "."), (0, 1, 1));
}

/// A qualified import of a repository's Scala code never claims external: its
/// imports are linked by scope, not file, so no outcome names a package.
#[test]
fn scala_imports_below_the_scan_root_claim_no_package() {
    let temp = fixture(&[
        (
            "svc/src/acme/B.scala",
            "package acme\nobject B { def go(): Int = 1 }\n",
        ),
        (
            "svc/src/acme/app/A.scala",
            "package acme.app\nimport acme.B\nobject A { def run(): Int = B.go() }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(tally(root, ".").1, 0);
    assert_eq!(
        caller_edges(root, "svc/src/acme/B.scala#B.go")
            .iter()
            .map(|(id, _, _)| id.as_str())
            .collect::<Vec<_>>(),
        ["svc/src/acme/app/A.scala#A.run"]
    );
}
