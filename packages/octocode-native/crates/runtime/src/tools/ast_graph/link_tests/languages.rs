//! Go, Python and Java package linking.
use super::*;

// ── Go, Python, Java ─────────────────────────────────────────────────────────

#[test]
fn go_package_imports_link_every_package_file_with_its_import_line() {
    let temp = fixture(&[
        ("go.mod", "module example.com/app\n"),
        (
            "main.go",
            "package main\n\nimport (\n\t\"fmt\"\n\t\"example.com/app/tsdb\"\n)\n\nfunc main() { fmt.Println(tsdb.Open()) }\n",
        ),
        (
            "tsdb/db.go",
            "package tsdb\n\nfunc Open() int { return 1 }\n",
        ),
        ("tsdb/head.go", "package tsdb\n\ntype Head struct{}\n"),
        ("tsdb/db_test.go", "package tsdb\n"),
    ]);
    let root = ingested(&temp);
    let deps = ask(root, "deps", Some("main.go"));
    assert_eq!(files(&deps), ["tsdb/db.go", "tsdb/head.go"], "{deps}");
    for file in ["tsdb/db.go", "tsdb/head.go"] {
        assert_eq!(result_row(&deps, file)["edgeLine"], 5, "{deps}");
    }
    let dependents = ask(root, "dependents", Some("tsdb/head.go"));
    assert_eq!(files(&dependents), ["main.go"], "{dependents}");
}

/// A scan rooted below the module root still reads the enclosing `go.mod`.
#[test]
fn go_subdirectory_roots_resolve_through_the_enclosing_go_mod() {
    let temp = fixture(&[
        ("go.mod", "module example.com/app\n"),
        (
            "tsdb/chunkenc/chunk.go",
            "package chunkenc\n\nfunc New() int { return 1 }\n",
        ),
        (
            "tsdb/db.go",
            "package tsdb\n\nimport \"example.com/app/tsdb/chunkenc\"\n\nfunc Open() int { return chunkenc.New() }\n",
        ),
    ]);
    let root = temp.0.as_path();
    ingest(root, &root.join("tsdb"));
    let dependents = ask(root, "dependents", Some("chunkenc/chunk.go"));
    assert_eq!(files(&dependents), ["db.go"], "{dependents}");
}

/// A scan rooted at a Python package resolves absolute imports that name
/// that package (`from pkg.utils.text import x` under root `pkg/`).
#[test]
fn python_package_roots_resolve_imports_through_their_package_name() {
    let temp = fixture(&[
        ("pkg/__init__.py", ""),
        ("pkg/utils/__init__.py", ""),
        ("pkg/utils/text.py", "def slug(x):\n    return x\n"),
        (
            "pkg/admin.py",
            "from pkg.utils.text import slug\n\nprint(slug('a'))\n",
        ),
    ]);
    let root = temp.0.as_path();
    ingest(root, &root.join("pkg"));
    let dependents = ask(root, "dependents", Some("utils/text.py"));
    assert_eq!(files(&dependents), ["admin.py"], "{dependents}");
}

/// Java classes of one package use each other without an import; the
/// edge cites the first use, an imported class the import.
#[test]
fn java_same_package_and_imported_classes_link_to_their_files() {
    let temp = fixture(&[
        (
            "src/com/acme/Lists.java",
            "package com.acme;\n\npublic final class Lists {\n  public static int transform(int x) { return x; }\n}\n",
        ),
        (
            "src/com/acme/util/Strings.java",
            "package com.acme.util;\n\npublic class Strings {}\n",
        ),
        (
            "src/com/acme/Multimaps.java",
            "package com.acme;\n\nimport com.acme.util.Strings;\nimport java.util.List;\n\nfinal class Multimaps {\n  int run() { return Lists.transform(1); }\n}\n",
        ),
        (
            "src/com/acme/Other.java",
            "package com.acme;\n\nfinal class Other {\n  int run() { return 2; }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let dependents = ask(root, "dependents", Some("src/com/acme/Lists.java"));
    assert_eq!(
        files(&dependents),
        ["src/com/acme/Multimaps.java"],
        "{dependents}"
    );
    let deps = ask(root, "deps", Some("src/com/acme/Multimaps.java"));
    let same = result_row(&deps, "src/com/acme/Lists.java");
    assert_eq!(same["via"], "java-same-package", "{deps}");
    assert_eq!(same["edgeLine"], 7, "{deps}");
    let imported = result_row(&deps, "src/com/acme/util/Strings.java");
    assert_eq!(imported["via"], "java-import", "{deps}");
    assert_eq!(imported["edgeLine"], 3, "{deps}");
}
