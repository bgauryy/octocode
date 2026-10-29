//! Go name resolution as gopls and scip-go model it: a bare name is the
//! package's (its directory's files, or a dot import's); `pkg.F` reads the
//! import the file binds under that name (its alias, if any); `x.M()` reads
//! the method set of `x`'s type, including methods promoted from embedded
//! fields; an interface method call stays on the interface.
use super::*;

const GO_MOD: (&str, &str) = ("go.mod", "module example.com/app\n");

const STORE: (&str, &str) = (
    "store/store.go",
    "package store\n\ntype base struct{}\n\nfunc (b base) Ping() int { return 1 }\n\ntype Store struct {\n\tbase\n}\n\nfunc (s *Store) Get(k string) int { return s.helper() }\n\ntype Getter interface {\n\tGet(k string) int\n}\n\nfunc Use(g Getter) int { return g.Get(\"x\") }\n",
);

/// A method set may span the package's files.
const STORE_HELPER: (&str, &str) = (
    "store/helper.go",
    "package store\n\nfunc (s Store) helper() int { return 2 }\n\nfunc New() *Store { return &Store{} }\n",
);

#[test]
fn go_aliased_dot_and_blank_imports_bind_what_go_binds() {
    let temp = fixture(&[
        GO_MOD,
        STORE,
        STORE_HELPER,
        ("dot/dot.go", "package dot\n\nfunc DotFn() {}\n"),
        ("side/side.go", "package side\n\nfunc Init() {}\n"),
        (
            "main.go",
            "package main\n\nimport (\n\tst \"example.com/app/store\"\n\t. \"example.com/app/dot\"\n\t_ \"example.com/app/side\"\n)\n\nfunc aliased() { st.New() }\n\nfunc unbound() { store.New(); side.Init() }\n\nfunc dotted() { DotFn() }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "main.go#aliased"),
        [edge("store/helper.go#New", "namespace", "high")]
    );
    assert_eq!(call_edges(root, "main.go#unbound"), []);
    assert_eq!(
        call_edges(root, "main.go#dotted"),
        [edge("dot/dot.go#DotFn", "import-glob", "high")]
    );
    // The blank import still loads its package.
    let deps = ask(root, "deps", Some("main.go"));
    assert!(files(&deps).contains(&"side/side.go"), "{deps}");
}

/// A bare name is the package's own: never another package's declaration,
/// exported or not, imported or not, and never a method.
#[test]
fn go_bare_names_bind_only_the_callers_package() {
    let temp = fixture(&[
        GO_MOD,
        STORE,
        STORE_HELPER,
        ("other/run.go", "package other\n\nfunc runner() {}\n"),
        (
            "main.go",
            "package main\n\nimport \"example.com/app/store\"\n\nfunc main() { runner(); New(); Get(\"k\"); local() }\n",
        ),
        ("local.go", "package main\n\nfunc local() {}\n"),
        (
            "store/util.go",
            "package store\n\nfunc sibling() *Store { return New() }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "main.go#main"),
        [edge("local.go#local", "same-package", "high")]
    );
    assert_eq!(
        call_edges(root, "store/util.go#sibling"),
        [edge("store/helper.go#New", "same-package", "high")]
    );
}

/// Value and pointer receivers share one method set, across the package's
/// files; the receiver's type is read from the receiver parameter, a
/// declaration or a composite literal. A `pkg.T` receiver binds `T` through
/// the import, as `pkg.F()` does (namespace confidence).
#[test]
fn go_method_calls_bind_the_receiver_types_method_set() {
    let temp = fixture(&[
        GO_MOD,
        STORE,
        STORE_HELPER,
        (
            "main.go",
            "package main\n\nimport \"example.com/app/store\"\n\nfunc declared() { var t store.Store; t.Get(\"a\") }\n\nfunc literal() { s := &store.Store{}; s.Get(\"b\") }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "store/store.go#Store.Get"),
        [edge(
            "store/helper.go#Store.helper",
            "receiver-type",
            "high"
        )]
    );
    for caller in ["main.go#declared", "main.go#literal"] {
        assert_eq!(
            call_edges(root, caller),
            [edge("store/store.go#Store.Get", "receiver-type", "high")],
            "{caller}"
        );
    }
}

/// A method promoted from an embedded field is the embedded type's; the
/// outer type's own method of that name shadows it.
#[test]
fn go_embedded_fields_promote_their_methods() {
    let temp = fixture(&[
        GO_MOD,
        STORE,
        STORE_HELPER,
        (
            "store/wrap.go",
            "package store\n\ntype Wrapped struct {\n\t*Store\n}\n\nfunc (w Wrapped) Get(k string) int { return 0 }\n\nfunc use() {\n\tvar s Store\n\ts.Ping()\n\tvar w Wrapped\n\tw.Ping()\n\tw.Get(\"k\")\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "store/wrap.go#use"),
        [
            edge("store/store.go#base.Ping", "inherited", "high"),
            edge("store/wrap.go#Wrapped.Get", "receiver-type", "high"),
        ]
    );
}

/// An interface method call names the interface's method (or the one an
/// embedded interface declares), never one of its implementations.
#[test]
fn go_interface_calls_stay_on_the_interface() {
    let temp = fixture(&[
        GO_MOD,
        STORE,
        STORE_HELPER,
        (
            "store/rw.go",
            "package store\n\ntype Pinger interface {\n\tGetter\n\tPing() int\n}\n\nfunc both(p Pinger) int { return p.Get(\"k\") + p.Ping() }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "store/store.go#Use"),
        [edge("store/store.go#Getter.Get", "receiver-type", "high")]
    );
    assert_eq!(
        call_edges(root, "store/rw.go#both"),
        [
            edge("store/rw.go#Pinger.Ping", "receiver-type", "high"),
            edge("store/store.go#Getter.Get", "inherited", "high"),
        ]
    );
}

/// Generic functions and methods of generic types bind like any other.
#[test]
fn go_generic_functions_and_methods_link() {
    let temp = fixture(&[
        GO_MOD,
        (
            "list/list.go",
            "package list\n\ntype List[T any] struct{ items []T }\n\nfunc (l *List[T]) Push(v T) { l.items = append(l.items, v) }\n\nfunc Map[T any](xs []T, f func(T) T) []T { return xs }\n",
        ),
        (
            "main.go",
            "package main\n\nimport \"example.com/app/list\"\n\nfunc main() {\n\tl := &list.List[int]{}\n\tl.Push(1)\n\tlist.Map[int]([]int{1}, nil)\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "main.go#main"),
        [
            edge("list/list.go#List.Push", "receiver-type", "high"),
            edge("list/list.go#Map", "namespace", "high"),
        ]
    );
}

/// An internal test file is the package; an external `_test` package
/// reaches it through its import. A non-test file never sees test files.
#[test]
fn go_test_packages_bind_like_go_test_builds_them() {
    let temp = fixture(&[
        GO_MOD,
        STORE,
        STORE_HELPER,
        (
            "store/internal_test.go",
            "package store\n\nimport \"testing\"\n\nfunc fixtureStore() *Store { return New() }\n\nfunc TestInternal(t *testing.T) { fixtureStore() }\n",
        ),
        (
            "store/external_test.go",
            "package store_test\n\nimport (\n\t\"testing\"\n\n\t\"example.com/app/store\"\n)\n\nfunc TestExternal(t *testing.T) { store.New() }\n",
        ),
        (
            "store/prod.go",
            "package store\n\nfunc prod() { fixtureStore() }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "store/internal_test.go#TestInternal"),
        [edge("store/internal_test.go#fixtureStore", "local", "high")]
    );
    assert_eq!(
        call_edges(root, "store/internal_test.go#fixtureStore"),
        [edge("store/helper.go#New", "same-package", "medium")]
    );
    assert_eq!(
        call_edges(root, "store/external_test.go#TestExternal"),
        [edge("store/helper.go#New", "namespace", "high")]
    );
    assert_eq!(call_edges(root, "store/prod.go#prod"), []);
}
