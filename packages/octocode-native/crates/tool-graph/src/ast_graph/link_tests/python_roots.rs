//! Where Python's absolute imports start from, as `sys.path` and pytest's
//! `rootdir` import read it, and what an import that links nowhere is called.
use super::module_roots::{reasons, row, tally};
use super::*;

/// A script's directory is on `sys.path`: the package beside it links, for a
/// plain import, an alias and a re-export, with no false reason left behind.
#[test]
fn python_finds_the_package_beside_a_script_below_the_scan_root() {
    let temp = fixture(&[
        ("py/pkg/__init__.py", "from .core import helper\n"),
        ("py/pkg/core.py", "def helper():\n    return 3\n"),
        (
            "py/use.py",
            "import pkg.core as pc\nfrom pkg import helper as h\ndef run():\n    h()\n    pc.helper()\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "py/pkg/core.py#helper"),
        [edge("py/use.py#run", "import", "high")]
    );
    assert_eq!(tally(root, "."), (3, 0, 0));
    assert_eq!(reasons(root, "."), []);
}

/// A package's own directory is not on `sys.path`: `import json` inside the
/// package holding `json.py` is the standard library's, as Python 3 reads it.
#[test]
fn python_module_in_a_package_does_not_shadow_an_absolute_import() {
    let temp = fixture(&[
        ("app/__init__.py", ""),
        ("app/core/__init__.py", ""),
        ("app/core/json.py", "def dumps():\n    return 1\n"),
        (
            "app/core/use.py",
            "import json\ndef run():\n    json.dumps()\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(call_edges(root, "app/core/use.py#run"), []);
    assert_eq!(tally(root, "."), (0, 1, 0));
    assert_eq!(reasons(root, "."), [row("json.dumps", "external-package")]);
}

/// The directory above a topmost package is a root for any file of the scan,
/// when only one package has that name.
#[test]
fn python_sole_topmost_package_links_from_another_tree() {
    let temp = fixture(&[
        ("python/lib/__init__.py", ""),
        ("python/lib/mod.py", "def f():\n    return 1\n"),
        (
            "tests/test_mod.py",
            "from lib.mod import f\ndef test():\n    f()\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "python/lib/mod.py#f"),
        [edge("tests/test_mod.py#test", "import", "high")]
    );
}

/// Two packages of one name: the import names code here, and which is unknown.
#[test]
fn python_two_candidates_are_neither_guessed_nor_external() {
    let temp = fixture(&[
        ("one/utils/__init__.py", "def f():\n    return 1\n"),
        ("two/utils/__init__.py", "def f():\n    return 2\n"),
        ("tool/use.py", "from utils import f\ndef run():\n    f()\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(call_edges(root, "tool/use.py#run"), []);
    assert_eq!(tally(root, "."), (0, 0, 1));
    assert_eq!(reasons(root, "."), [row("f", "other")]);
}

/// A namespace package (no `__init__.py`) under a directory nothing names as a
/// root: the import matches a file, so it is an unknown, never external.
#[test]
fn python_namespace_module_is_not_called_external() {
    let temp = fixture(&[
        ("a/ns/mod.py", "def f():\n    return 1\n"),
        ("b/use.py", "from ns.mod import f\ndef run():\n    f()\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(tally(root, "."), (0, 0, 1));
    assert_eq!(reasons(root, "."), [row("f", "other")]);
}

/// A standard-library name stays the library's even when some other tree holds
/// a topmost package by that name; the importer's own root would still shadow.
#[test]
fn python_standard_module_is_not_bound_to_another_tree() {
    let temp = fixture(&[
        ("svc/logging/__init__.py", "def info():\n    return 1\n"),
        (
            "scripts/run.py",
            "import logging\ndef run():\n    logging.info()\n",
        ),
        ("own/json.py", "def dumps():\n    return 1\n"),
        ("own/use.py", "import json\ndef run():\n    json.dumps()\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(call_edges(root, "scripts/run.py#run"), []);
    assert_eq!(
        call_edges(root, "own/use.py#run"),
        [edge("own/json.py#dumps", "namespace", "high")]
    );
    assert_eq!(tally(root, ".").1, 1);
}

/// The standard library's private modules (`_socket`, `sre_parse`) are the
/// library's too: the name list comes from the interpreter and typeshed, not
/// from the public names someone remembered.
#[test]
fn python_private_standard_module_is_not_bound_to_another_tree() {
    let temp = fixture(&[
        ("svc/_socket/__init__.py", "def info():\n    return 1\n"),
        (
            "scripts/run.py",
            "import _socket\ndef run():\n    _socket.info()\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(call_edges(root, "scripts/run.py#run"), []);
}

/// What the scan root's enclosing package holds beside the root is repository
/// code outside the scope, and so is a module the directories above spell.
#[test]
fn python_module_above_the_scan_root_is_unscanned_not_external() {
    let temp = fixture(&[
        (".git/HEAD", "ref: refs/heads/main\n"),
        ("tool/__init__.py", ""),
        ("tool/other.py", "def o():\n    return 1\n"),
        ("tool/sub/__init__.py", ""),
        (
            "tool/sub/a.py",
            "from tool.other import o\nfrom shared.util import u\nimport numpy\ndef a():\n    o()\n    u()\n    numpy.zeros()\n",
        ),
        ("shared/util.py", "def u():\n    return 1\n"),
    ]);
    let root = temp.0.as_path();
    assert_eq!(
        ingest(root, &root.join("tool/sub")).value.get("error"),
        None
    );
    assert_eq!(tally(root, "tool/sub"), (0, 1, 2));
    assert_eq!(
        reasons(root, "tool/sub"),
        [
            row("numpy.zeros", "external-package"),
            row("o", "other"),
            row("u", "other")
        ]
    );
}

/// A scan root's own name: `import app.core` under a scan of `app/`, package or not.
#[test]
fn python_import_naming_the_scan_root_links_inside_it() {
    let temp = fixture(&[
        (".git/HEAD", "ref: refs/heads/main\n"),
        ("app/core.py", "def f():\n    return 1\n"),
        (
            "app/use.py",
            "from app.core import f\ndef run():\n    f()\n",
        ),
    ]);
    let root = temp.0.as_path();
    assert_eq!(ingest(root, &root.join("app")).value.get("error"), None);
    assert_eq!(tally(root, "app"), (1, 0, 0));
}
