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

/// The specifiers of the `unresolved-import` findings of the snapshot at
/// `scope`, `(outside the root, broken)`, each sorted. Whatever the language,
/// an outside-root site's message is its quoted specifier, its finding ranks
/// below a broken one, and every site is one `unresolvedInternal` import.
pub(super) fn unresolved_imports(root: &Path, scope: &str) -> (Vec<String>, Vec<String>) {
    let issues = ask_with(root, "issues", None, |o| {
        o.path = Some(scope.into());
        o.detectors = vec!["unresolved-import".into()];
        o.min_tier = Some(0);
    });
    let (mut outside, mut broken) = (Vec::new(), Vec::new());
    let (mut outside_confidence, mut broken_confidence) = (Vec::new(), Vec::new());
    let mut count = 0;
    for row in &crate::tools::ast_graph::test_support::issue_rows(&issues) {
        let is_outside = row["evidence"]["outsideRoot"] == true;
        count += row["evidence"]["count"].as_u64().unwrap_or(0);
        let confidence = row["confidence"].as_f64().unwrap_or(f64::NAN);
        let (specs, confidences) = if is_outside {
            (&mut outside, &mut outside_confidence)
        } else {
            (&mut broken, &mut broken_confidence)
        };
        confidences.push(confidence);
        for site in row["evidence"]["sites"].as_array().into_iter().flatten() {
            let message = site["message"].as_str().unwrap_or_default();
            // An outside import's message is its quoted specifier alone.
            assert_eq!(
                message.starts_with('"') && message.ends_with('"'),
                is_outside,
                "{issues}"
            );
            specs.extend(message.split('"').nth(1).map(str::to_owned));
        }
    }
    assert!(
        outside_confidence
            .iter()
            .all(|out| broken_confidence.iter().all(|gap| out < gap)),
        "a scope gap ranks below a broken import: {issues}"
    );
    assert_eq!(count, tally(root, scope).2, "{issues}");
    outside.sort();
    broken.sort();
    (outside, broken)
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
    // Both unlinked imports name code above the scan.
    assert_eq!(
        unresolved_imports(root, "web/src"),
        (vec!["other/x".into(), "~shared/y".into()], vec![])
    );
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

/// A bare specifier whose code the project places beyond the scan root (a
/// workspace sibling, a `file:` dependency, a `paths` alias to a directory
/// above) is the same scope gap as a relative import climbing out of the root:
/// an outside-root `unresolved-import` finding, never a broken import. An
/// alias into the scan that finds nothing stays broken, and every unlinked
/// project import is counted `unresolvedInternal` either way.
#[test]
fn typescript_bare_imports_of_code_beyond_the_scan_are_outside_the_root() {
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
        ("shared/y.ts", "export function y() { return 1; }\n"),
        (
            "packages/app/tsconfig.json",
            r#"{"compilerOptions":{"paths":{"~shared/*":["../../shared/*"],"@app/*":["./src/*"]}}}"#,
        ),
        (
            "packages/app/package.json",
            r#"{"name":"app","dependencies":{"@acme/lib":"workspace:*","linked":"file:../../linked","react":"^19"}}"#,
        ),
        (
            "packages/app/src/use.ts",
            "import { f } from \"@acme/lib\";\nimport { l } from \"linked\";\nimport { y } from \"~shared/y\";\nimport { gone } from \"@app/gone\";\nimport { r } from \"react\";\nexport async function run() { const lib = await import(\"@acme/lib\"); return [f(), l(), y(), gone, r(), lib]; }\n",
        ),
    ]);
    let root = temp.0.as_path();
    let scope = "packages/app";
    assert_eq!(ingest(root, &root.join(scope)).value.get("error"), None);
    assert_eq!(
        unresolved_imports(root, scope),
        (
            vec!["@acme/lib".into(), "linked".into(), "~shared/y".into()],
            vec!["@app/gone".into()]
        )
    );
    // Five project imports and one package import; where each lies does not
    // move the tally.
    assert_eq!(tally(root, scope), (0, 1, 5));
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

/// Another package of the module enclosing the scan, a `go.work` member and a
/// local `replace` lie beyond the scan root: outside-root findings. A package
/// path inside the scan that holds no file stays a broken import.
#[test]
fn go_modules_beyond_the_scan_are_outside_the_root() {
    let temp = fixture(&[
        (".git/HEAD", "ref: refs/heads/main\n"),
        ("go.work", "go 1.22\n\nuse (\n\t./svc\n\t./mem\n)\n"),
        ("mem/go.mod", "module x.io/mem\n"),
        ("mem/u/u.go", "package u\n\nfunc U() {}\n"),
        (
            "svc/go.mod",
            "module x.io/m\n\nreplace x.io/rep => ../rep\n",
        ),
        ("svc/b/b.go", "package b\n\nfunc B() {}\n"),
        (
            "svc/a/a.go",
            "package a\n\nimport (\n\t\"fmt\"\n\t\"x.io/m/a/gone\"\n\t\"x.io/m/b\"\n\t\"x.io/mem/u\"\n\t\"x.io/rep/v\"\n)\n\nfunc A() {\n\tb.B()\n\tu.U()\n\tv.V()\n\tgone.G()\n\tfmt.Println()\n}\n",
        ),
    ]);
    let root = temp.0.as_path();
    assert_eq!(ingest(root, &root.join("svc/a")).value.get("error"), None);
    assert_eq!(
        unresolved_imports(root, "svc/a"),
        (
            vec!["x.io/m/b".into(), "x.io/mem/u".into(), "x.io/rep/v".into()],
            vec!["x.io/m/a/gone".into()]
        )
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
    // The class above the scan is beyond the root; the one the scan's own
    // package lacks is broken.
    assert_eq!(
        unresolved_imports(root, "src/com/acme/app"),
        (vec!["com.acme.B".into()], vec!["com.acme.app.Gone".into()])
    );
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

/// A path dependency (a workspace member's too) whose package lies beyond the
/// scan root is an outside-root finding, not a broken import.
#[test]
fn rust_path_dependencies_beyond_the_scan_are_outside_the_root() {
    let temp = fixture(&[
        (".git/HEAD", "ref: refs/heads/main\n"),
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n\n[workspace.dependencies]\nmember = { path = \"crates/member\" }\n",
        ),
        (
            "crates/member/Cargo.toml",
            "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("crates/member/src/lib.rs", "pub fn m() {}\n"),
        (
            "crates/lib/Cargo.toml",
            "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("crates/lib/src/lib.rs", "pub fn f() {}\n"),
        (
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nmylib = { path = \"../lib\" }\nmember = { workspace = true }\n",
        ),
        (
            "crates/app/src/main.rs",
            "use mylib::f;\nuse member::m;\nfn main() {\n    f();\n    m();\n}\n",
        ),
    ]);
    let root = temp.0.as_path();
    assert_eq!(
        ingest(root, &root.join("crates/app")).value.get("error"),
        None
    );
    assert_eq!(
        unresolved_imports(root, "crates/app"),
        (vec!["member::m".into(), "mylib::f".into()], vec![])
    );
}

/// A Python module beyond the scan root (a local path dependency the project's
/// `pyproject.toml` names, src layout or flat; a module a directory above the
/// scan spells; another module of the package enclosing the scan) is an
/// outside-root finding. A module of the scan's own package that is missing
/// stays a broken import.
#[test]
fn python_modules_beyond_the_scan_are_outside_the_root() {
    let temp = fixture(&[
        (".git/HEAD", "ref: refs/heads/main\n"),
        (
            "libs/uvlib/src/uv_pkg/__init__.py",
            "def u():\n    return 1\n",
        ),
        ("libs/poetrylib/poetry_mod.py", "def p():\n    return 1\n"),
        (
            "libs/pep508/pep_pkg/__init__.py",
            "def q():\n    return 1\n",
        ),
        ("proj/shared/util.py", "def s():\n    return 1\n"),
        ("proj/pkg/__init__.py", ""),
        ("proj/pkg/other.py", "def o():\n    return 1\n"),
        ("proj/pkg/app/__init__.py", ""),
        (
            "proj/pkg/app/pyproject.toml",
            "[project]\nname = \"app\"\ndependencies = [\"uvlib\", \"pep508 @ file:../../../libs/pep508\", \"requests\"]\n\n[tool.uv.sources]\nuvlib = { path = \"../../../libs/uvlib\", editable = true }\n\n[tool.poetry.dependencies]\npoetrylib = { path = \"../../../libs/poetrylib\", develop = true }\n",
        ),
        (
            "proj/pkg/app/main.py",
            "import uv_pkg\nimport poetry_mod\nimport pep_pkg\nimport shared.util\nimport pkg.other\nimport pkg.app.gone\nimport requests\n",
        ),
    ]);
    let root = temp.0.as_path();
    let scope = "proj/pkg/app";
    assert_eq!(ingest(root, &root.join(scope)).value.get("error"), None);
    assert_eq!(
        unresolved_imports(root, scope),
        (
            vec![
                "pep_pkg".into(),
                "pkg.other".into(),
                "poetry_mod".into(),
                "shared.util".into(),
                "uv_pkg".into()
            ],
            vec!["pkg.app.gone".into()]
        )
    );
    assert_eq!(tally(root, scope), (0, 1, 6));
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
