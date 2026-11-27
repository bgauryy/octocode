//! A file that spells the name is a missed site only if it could answer the
//! question: an override is no call of what it overrides, and a call is no
//! implementation.
use super::super::super::scope::Question;
use super::*;
use octocode_test_support::json_fixture;

/// `scope` over `files` after the server answered `answered`, asking what
/// `op` asks.
fn answered_scope(root: &Path, op: Op, name: &str, answered: &str) -> Scope {
    let text = root.to_string_lossy().into_owned();
    let extension = Path::new(answered)
        .extension()
        .and_then(|extension| extension.to_str())
        .expect("extension")
        .to_owned();
    let scope = scanned(Scope::new(text, vec![format!("*.{extension}")]), name);
    scope.asks(Question::of(op));
    scope.answer([root.join(answered).to_string_lossy()]);
    scope
}

fn files(root: &Path, files: &[(&str, &str)]) {
    for (name, content) in files {
        std::fs::write(root.join(name), content).expect("file");
    }
}

/// The audit's row: a Python subclass overrides the method and nothing calls
/// it there, so the row is not partial for that file.
#[test]
fn a_python_override_alone_is_no_missed_call() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    files(
        &root,
        &[
            (
                "port.py",
                "class Base:\n    def run(self, at):\n        return at\n",
            ),
            (
                "ledger.py",
                "from port import Base\nclass Derived(Base):\n    def run(self, at):\n        return at\n",
            ),
        ],
    );
    let scope = answered_scope(&root, Op::Callers, "run", "port.py");
    let mut row = json_fixture(
        r#"{
            "operation": "callers",
            "files": [{"path": "use.py", "matches": [{"line": 1}]}]
        }"#,
    );
    let q = query(
        json!({"path":root.join("port.py").to_string_lossy(),"operation":"callers","symbolName":"run","line":2}),
    );
    annotate(
        &mut row,
        &q,
        Some("python"),
        Server {
            anchor: "port.py",
            root: ".",
        },
        &scope,
    );
    assert_eq!(row.get("isPartial"), None, "{row}");
    assert_eq!(row["coverage"]["textOnlyFiles"], 0, "{row}");
}

/// A use beside the override is still a missed call, and a file the answer
/// leaves out that only calls through a receiver stays the dynamic miss.
#[test]
fn a_python_use_beside_an_override_is_still_missed() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    files(
        &root,
        &[
            (
                "port.py",
                "class Base:\n    def run(self, at):\n        return at\n",
            ),
            (
                "ledger.py",
                "class Derived:\n    def run(self, at):\n        return self.inner.run(at)\n",
            ),
        ],
    );
    let scope = answered_scope(&root, Op::Callers, "run", "port.py");
    let mut row = json_fixture(
        r#"{
            "operation": "callers",
            "files": [{"path": "use.py", "matches": [{"line": 1}]}]
        }"#,
    );
    let q = query(
        json!({"path":root.join("port.py").to_string_lossy(),"operation":"callers","symbolName":"run","line":2}),
    );
    annotate(
        &mut row,
        &q,
        Some("python"),
        Server {
            anchor: "port.py",
            root: ".",
        },
        &scope,
    );
    assert_eq!(row["partialReasons"], json!([DYNAMIC_REASON]), "{row}");
    assert_eq!(row["coverage"]["textOnlyFiles"], 1, "{row}");
}

/// A file that only calls the method holds no implementation of it: the Rust
/// row no longer counts it next to `state: complete`; a file that implements
/// it is still counted.
#[test]
fn a_rust_call_is_no_missed_implementation() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    files(
        &root,
        &[
            (
                "port.rs",
                "pub trait Port {\n    fn read(&self) -> u32;\n}\n",
            ),
            (
                "usage.rs",
                "pub fn use_it(p: &dyn Port) -> u32 {\n    p\n        .read()\n}\n",
            ),
        ],
    );
    let implementation = |answered: &str| {
        let scope = answered_scope(&root, Op::Implementation, "read", answered);
        let mut row = json!({"operation":"implementation","files":[{"path":answered,"matches":[{"line":1}]}]});
        let q = query(
            json!({"path":root.join("port.rs").to_string_lossy(),"operation":"implementation","symbolName":"read","line":2}),
        );
        annotate(
            &mut row,
            &q,
            Some("rust"),
            Server {
                anchor: "port.rs",
                root: ".",
            },
            &scope,
        );
        row
    };
    assert_eq!(implementation("port.rs")["coverage"]["textOnlyFiles"], 0);
    files(
        &root,
        &[(
            "other.rs",
            "impl Port for X {\n    fn read(&self) -> u32 { 1 }\n}\n",
        )],
    );
    assert_eq!(implementation("port.rs")["coverage"]["textOnlyFiles"], 1);
}

/// A type's implementations spell its name in `impl`, no call: they count.
#[test]
fn a_type_named_in_heritage_may_be_an_implementation() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    files(
        &root,
        &[
            ("port.rs", "pub trait Port {}\n"),
            ("other.rs", "impl Port for X {}\n"),
        ],
    );
    let scope = answered_scope(&root, Op::Implementation, "Port", "port.rs");
    let mut row = json_fixture(
        r#"{
            "operation": "implementation",
            "files": [{"path": "port.rs", "matches": [{"line": 1}]}]
        }"#,
    );
    let q = query(
        json!({"path":root.join("port.rs").to_string_lossy(),"operation":"implementation","symbolName":"Port","line":1}),
    );
    annotate(
        &mut row,
        &q,
        Some("rust"),
        Server {
            anchor: "port.rs",
            root: ".",
        },
        &scope,
    );
    assert_eq!(row["coverage"]["textOnlyFiles"], 1, "{row}");
}
