//! Python name resolution as pyright and scip-python model it: a module's
//! names are its own definitions and the names its imports bind (an
//! `__init__.py` re-exports what it imports); `self`/`cls`/`super()` bind
//! through the enclosing class and its bases; a bare name never reaches
//! another module without an import.
use super::*;

const UTIL: (&str, &str) = (
    "pkg/util.py",
    "def helper():\n    return 1\n\ndef _hidden():\n    return 2\n",
);

#[test]
fn python_relative_absolute_and_aliased_imports_bind_module_names() {
    let temp = fixture(&[
        ("pkg/__init__.py", ""),
        UTIL,
        (
            "pkg/core.py",
            "from . import util\nfrom .util import helper as h\nimport pkg.util as u\nimport pkg.util\n\ndef named():\n    return h()\n\ndef relative_module():\n    return util.helper()\n\ndef aliased_module():\n    return u.helper()\n\ndef dotted_module():\n    return pkg.util.helper()\n",
        ),
    ]);
    let root = ingested(&temp);
    let helper = "pkg/util.py#helper";
    assert_eq!(
        call_edges(root, "pkg/core.py#named"),
        [edge(helper, "import", "high")]
    );
    for caller in ["relative_module", "aliased_module", "dotted_module"] {
        assert_eq!(
            call_edges(root, &format!("pkg/core.py#{caller}")),
            [edge(helper, "namespace", "high")],
            "{caller}"
        );
    }
}

/// `from pkg import Engine` reaches the class `pkg/__init__.py` imports,
/// under its alias too; `import pkg; pkg.Engine()` reads the same binding.
#[test]
fn python_package_init_reexports_resolve_to_the_defining_module() {
    let temp = fixture(&[
        (
            "pkg/__init__.py",
            "from .core import Engine\nfrom .util import helper as public_helper\n__all__ = ['Engine', 'public_helper']\n",
        ),
        UTIL,
        ("pkg/core.py", "class Engine:\n    pass\n"),
        (
            "app.py",
            "import pkg\nfrom pkg import Engine, public_helper\n\ndef named():\n    Engine()\n    public_helper()\n\ndef through_package():\n    return pkg.Engine()\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "app.py#named"),
        [
            edge("pkg/core.py#Engine", "import", "high"),
            edge("pkg/util.py#helper", "import", "high"),
        ]
    );
    assert_eq!(
        call_edges(root, "app.py#through_package"),
        [edge("pkg/core.py#Engine", "namespace", "high")]
    );
}

/// `from m import *` binds `m`'s public names; an underscore name stays
/// private to `m` unless imported by name.
#[test]
fn python_star_imports_bind_public_names_only() {
    let temp = fixture(&[
        ("pkg/__init__.py", ""),
        UTIL,
        (
            "pkg/star.py",
            "from pkg.util import *\n\ndef run():\n    helper()\n    _hidden()\n",
        ),
        (
            "pkg/named.py",
            "from pkg.util import _hidden\n\ndef run():\n    _hidden()\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "pkg/star.py#run"),
        [edge("pkg/util.py#helper", "import-glob", "high")]
    );
    assert_eq!(
        call_edges(root, "pkg/named.py#run"),
        [edge("pkg/util.py#_hidden", "import", "high")]
    );
}

/// A bare name is a local, a module global, an imported name or a builtin:
/// a same-named function in a sibling module (or anywhere else) is a
/// different binding, at every confidence.
#[test]
fn python_bare_names_never_bind_another_module_without_an_import() {
    let temp = fixture(&[
        ("pkg/__init__.py", ""),
        ("pkg/sibling.py", "def sibling_only():\n    return 1\n"),
        ("far/unique.py", "def unique_somewhere():\n    return 2\n"),
        (
            "pkg/caller.py",
            "def run():\n    sibling_only()\n    unique_somewhere()\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(call_edges(root, "pkg/caller.py#run"), []);
    let unresolved = ask_with(root, "unresolved", Some("pkg/caller.py"), |o| {
        o.edges = vec!["calls".into()];
    });
    assert_eq!(unresolved["pagination"]["totalItems"], 2, "{unresolved}");
}

#[test]
fn python_self_cls_and_super_bind_through_the_class_and_its_bases() {
    let temp = fixture(&[(
        "shapes.py",
        "class Base:\n    def run(self):\n        return self.step()\n    def step(self):\n        return 1\n    @classmethod\n    def make(cls):\n        return cls.build()\n    @classmethod\n    def build(cls):\n        return 2\n\nclass Child(Base):\n    def step(self):\n        return super().step()\n    def again(self):\n        return self.step() + self.run()\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "shapes.py#Base.run"),
        [edge("shapes.py#Base.step", "local", "high")]
    );
    assert_eq!(
        call_edges(root, "shapes.py#Base.make"),
        [edge("shapes.py#Base.build", "local", "high")]
    );
    assert_eq!(
        call_edges(root, "shapes.py#Child.step"),
        [edge("shapes.py#Base.step", "inherited", "high")]
    );
    assert_eq!(
        call_edges(root, "shapes.py#Child.again"),
        [
            edge("shapes.py#Base.run", "inherited", "high"),
            edge("shapes.py#Child.step", "local", "high"),
        ]
    );
}

/// `Engine()` instantiates the class; a constructed or annotated value
/// calls the class's methods, chained or through a local.
#[test]
fn python_instantiation_nested_functions_and_typed_receivers() {
    let temp = fixture(&[
        ("pkg/__init__.py", ""),
        (
            "pkg/engine.py",
            "class Engine:\n    def __init__(self, name):\n        self.name = name\n    def go(self):\n        def inner():\n            return 3\n        return inner()\n",
        ),
        (
            "pkg/app.py",
            "from pkg.engine import Engine\n\ndef build():\n    return Engine('a')\n\ndef local_receiver():\n    e = Engine('b')\n    return e.go()\n\ndef annotated(e: Engine):\n    return e.go()\n\ndef chained():\n    return Engine('c').go()\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "pkg/engine.py#Engine.go"),
        [edge("pkg/engine.py#Engine.go.inner", "local", "high")]
    );
    assert_eq!(
        call_edges(root, "pkg/app.py#build"),
        [edge("pkg/engine.py#Engine", "import", "high")]
    );
    assert_eq!(
        call_edges(root, "pkg/app.py#local_receiver"),
        [
            edge("pkg/engine.py#Engine", "import", "high"),
            edge("pkg/engine.py#Engine.go", "receiver-type", "high"),
        ]
    );
    assert_eq!(
        call_edges(root, "pkg/app.py#annotated"),
        [edge("pkg/engine.py#Engine.go", "receiver-type", "high")]
    );
    assert_eq!(
        call_edges(root, "pkg/app.py#chained"),
        [
            edge("pkg/engine.py#Engine", "import", "high"),
            edge("pkg/engine.py#Engine.go", "receiver-type", "high"),
        ]
    );
}

/// `@deco` applies `deco` to the function it decorates when the enclosing
/// scope (here the module) runs; `@deco(arg)` calls `deco` first.
#[test]
fn python_decorators_are_applied_calls() {
    let temp = fixture(&[
        ("pkg/__init__.py", ""),
        (
            "pkg/deco.py",
            "def traced(f):\n    return f\n\ndef route(path):\n    return traced\n",
        ),
        (
            "pkg/views.py",
            "from pkg.deco import traced, route\n\n@traced\ndef plain():\n    return 1\n\n@route('/x')\ndef with_args():\n    return 2\n",
        ),
    ]);
    let root = ingested(&temp);
    let deps = ask_with(root, "callers", Some("pkg/deco.py#traced"), |o| {
        o.confidence = Some("high".into());
    });
    assert_eq!(result_ids(&deps), ["pkg/views.py"], "{deps}");
    let deps = ask_with(root, "callers", Some("pkg/deco.py#route"), |o| {
        o.confidence = Some("high".into());
    });
    assert_eq!(result_ids(&deps), ["pkg/views.py"], "{deps}");
}

/// Imports under `if TYPE_CHECKING:` never run: they close no runtime
/// import cycle.
#[test]
fn python_type_checking_imports_are_not_runtime_cycles() {
    let temp = fixture(&[
        ("pkg/__init__.py", ""),
        (
            "pkg/a.py",
            "from pkg.b import B\n\nclass A:\n    def b(self) -> B:\n        return B()\n",
        ),
        (
            "pkg/b.py",
            "from typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    from pkg.a import A\n\nclass B:\n    def a(self, x: 'A'):\n        return x\n",
        ),
    ]);
    let root = ingested(&temp);
    let cycles = ask(root, "cycles", None);
    let rows = cycles["rows"].as_array().cloned().unwrap_or_default();
    assert!(rows.iter().all(|row| row["runtime"] == false), "{cycles}");
}
