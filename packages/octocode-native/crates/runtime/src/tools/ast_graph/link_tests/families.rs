//! Graph-wide name links stay inside one language family: a call, base
//! type, constructor or passed reference never binds to a same-named
//! declaration of another language, while interop families (JS/TS, C/C++)
//! still link.
use super::*;

fn neighbours(root: &Path, op: &str, target: &str, edges: &[&str]) -> Vec<String> {
    let out = ask_with(root, op, Some(target), |o| {
        o.confidence = Some("low".into());
        o.edges = edges.iter().map(|edge| (*edge).to_owned()).collect();
    });
    assert_eq!(out.get("error"), None, "{out}");
    let mut ids = result_ids(&out)
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn callers(root: &Path, target: &str) -> Vec<String> {
    neighbours(root, "callers", target, &[])
}

/// One directory, four languages: only the Python caller may reach the
/// Python function (same-package, import-scope and unique-name routes).
#[test]
fn calls_never_link_across_language_families() {
    let temp = fixture(&[
        (
            "b.py",
            "def only_in_python():\n    pass\n\ndef caller():\n    only_in_python()\n",
        ),
        ("a.ts", "export function main() { only_in_python(); }\n"),
        ("c.go", "package main\n\nfunc main() { only_in_python() }\n"),
        ("d.rs", "fn main() { only_in_python(); }\n"),
        (
            "far/e.ts",
            "export function remote() { only_in_python(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(callers(root, "b.py#only_in_python"), ["b.py#caller"]);
    let impact = ask(root, "impact", Some("b.py#only_in_python"));
    let affected = result_ids(&impact);
    for file in ["a.ts", "c.go", "d.rs", "far/e.ts"] {
        assert!(!affected.contains(&file), "{file} affected: {impact}");
    }
}

/// A name declared only in another family is not a repository target, so
/// the site is not counted as linkable.
#[test]
fn cross_family_names_are_not_counted_as_repository_targets() {
    let temp = fixture(&[
        ("b.py", "def only_in_python():\n    pass\n"),
        ("a.ts", "export function main() { only_in_python(); }\n"),
    ]);
    let root = ingested(&temp);
    let stats = ask(root, "stats", None);
    let typescript = &stats["linkageByLanguage"]["typescript"];
    assert_eq!(typescript["eligible"], 0, "{stats}");
}

/// Interop families still link without imports: JS ↔ TS and C ↔ C++.
#[test]
fn calls_link_inside_one_family() {
    let temp = fixture(&[
        ("web/util.js", "export function jsHelper() {}\n"),
        ("web/a.ts", "export function main() { jsHelper(); }\n"),
        (
            "far/b.tsx",
            "export function View() { jsHelper(); return null; }\n",
        ),
        ("native/impl.cpp", "int native_helper() { return 1; }\n"),
        ("native/main.c", "int main() { return native_helper(); }\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        callers(root, "web/util.js#jsHelper"),
        ["far/b.tsx#View", "web/a.ts#main"]
    );
    assert_eq!(
        callers(root, "native/impl.cpp#native_helper"),
        ["native/main.c#main"]
    );
}

/// Another family's same-named declaration no longer makes a name
/// ambiguous: the TypeScript caller links to the one JavaScript helper.
#[test]
fn other_families_do_not_make_a_name_ambiguous() {
    let temp = fixture(&[
        ("util.js", "export function shared_name() {}\n"),
        ("util.py", "def shared_name():\n    pass\n"),
        ("a.ts", "export function main() { shared_name(); }\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(callers(root, "util.js#shared_name"), ["a.ts#main"]);
    assert_eq!(callers(root, "util.py#shared_name"), Vec::<String>::new());
}

/// Base types, constructors and type-qualified calls stay in the family.
#[test]
fn types_and_members_never_link_across_language_families() {
    let temp = fixture(&[
        (
            "base.py",
            "class PyBase:\n    def run(self):\n        pass\n\n    @staticmethod\n    def make():\n        pass\n",
        ),
        (
            "a.ts",
            "export class Child extends PyBase {}\nexport function main() { PyBase.make(); return new PyBase(); }\n",
        ),
        ("base.js", "export class JsBase {}\n"),
        ("b.ts", "export class JsChild extends JsBase {}\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        neighbours(root, "dependents", "base.py#PyBase", &["inherits"]),
        Vec::<String>::new()
    );
    assert_eq!(callers(root, "base.py#PyBase.make"), Vec::<String>::new());
    assert_eq!(callers(root, "base.py#PyBase"), Vec::<String>::new());
    assert_eq!(
        neighbours(root, "dependents", "base.js#JsBase", &["inherits"]),
        ["b.ts#JsChild"]
    );
}

/// A passed reference binds only inside its family.
#[test]
fn passed_references_never_bind_across_language_families() {
    let temp = fixture(&[
        ("h.py", "def py_handler():\n    pass\n"),
        ("h.js", "export function jsHandler() {}\n"),
        (
            "a.ts",
            "function sink(f: unknown) {}\nexport function run() { sink(py_handler); sink(jsHandler); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        neighbours(root, "deps", "a.ts#run", &["passes"]),
        ["h.js#jsHandler"]
    );
}

/// A same-directory declaration of another language never outranks the
/// caller's own family, and typed receivers bind only their family's type.
#[test]
fn same_directory_foreign_declarations_never_win_a_choice() {
    let temp = fixture(&[
        (
            "sub/Foo.ts",
            "export class Foo { static bar() {} }\nexport function util() {}\n",
        ),
        (
            "a.ts",
            "export function main(x: Foo) { Foo.bar(); util(); x.bar(); }\n",
        ),
        (
            "a.py",
            "class Foo:\n    def bar(self):\n        pass\n\ndef util():\n    pass\n",
        ),
    ]);
    let root = ingested(&temp);
    let callees = neighbours(root, "callees", "a.ts#main", &[]);
    assert!(
        callees.iter().all(|id| !id.starts_with("a.py")),
        "{callees:?}"
    );
    assert!(
        callees.contains(&"sub/Foo.ts#util".to_owned()),
        "{callees:?}"
    );
}

/// C# and Java are separate families even in one package directory.
#[test]
fn csharp_and_java_never_link_to_each_other() {
    let temp = fixture(&[
        (
            "J.java",
            "class J {\n  static void JavaOnly() { Svc.Run(); }\n}\n",
        ),
        (
            "Svc.cs",
            "class Svc {\n  public static void Run() { J.JavaOnly(); }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(callers(root, "Svc.cs#Svc.Run"), Vec::<String>::new());
    assert_eq!(callers(root, "J.java#J.JavaOnly"), Vec::<String>::new());
}

/// Analyses built on calls inherit the boundary: no mixed-language call
/// cycle and no other language's tests to run.
#[test]
fn analyses_never_cross_language_families() {
    let temp = fixture(&[
        (
            "lib.ts",
            "export function tsUnused() { return pyBack(); }\n",
        ),
        ("lib2.py", "def pyBack():\n    return tsUnused()\n"),
        ("api.ts", "export function tsOnlyApi() { return 1; }\n"),
        (
            "tests/test_api.py",
            "def test_it():\n    assert tsOnlyApi() == 1\n",
        ),
    ]);
    let root = ingested(&temp);
    let cycles = ask_with(root, "cycles", None, |o| {
        o.edges = vec!["calls".into()];
        o.confidence = Some("low".into());
    });
    assert_eq!(cycle_members(&cycles), Vec::<Value>::new(), "{cycles}");
    let impact = ask(root, "impact", Some("api.ts"));
    assert_eq!(impact["summary"]["testsToRun"], json!([]), "{impact}");
}
