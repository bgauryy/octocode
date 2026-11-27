//! Every grammar credits a call to its innermost enclosing callable
//! declaration; a closure or lambda that is no declaration falls back to the
//! declaration holding it.

use super::*;

/// `(caller, callee)` of every call, with the caller's id naming it.
fn callers(source: &str, path: &str) -> Vec<(String, String)> {
    let facts: Value =
        serde_json::from_str(&extract_graph_facts(source, path).expect("facts")).expect("json");
    let ids = facts["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .map(|d| {
            (
                d["id"].as_str().unwrap().to_owned(),
                d["name"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    facts["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .map(|call| {
            let caller = call["caller"].as_str().unwrap().to_owned();
            if let Some(id) = call["callerId"].as_str() {
                assert_eq!(ids.get(id), Some(&caller), "{path}: {call}");
            }
            (caller, call["callee"].as_str().unwrap().to_owned())
        })
        .collect()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(caller, callee)| ((*caller).to_owned(), (*callee).to_owned()))
        .collect()
}

#[test]
fn nested_callables_own_their_calls_in_every_grammar() {
    for (path, source, expected) in [
        (
            "m.py",
            "def outer(src):\n    def helper(x):\n        return x + 1\n    def blocks(ls):\n        return helper(ls)\n    class Inner:\n        def run(self):\n            return helper(2)\n    blank = lambda l: check(l)\n    return blocks(src)\n",
            &[
                ("blocks", "helper"),
                ("run", "helper"),
                ("outer", "check"),
                ("outer", "blocks"),
            ][..],
        ),
        (
            "m.rs",
            "fn outer(src: u8) -> u8 {\n    fn helper(x: u8) -> u8 { x + 1 }\n    fn blocks(ls: u8) -> u8 { helper(ls) }\n    let blank = |l: u8| check(l);\n    blocks(src)\n}\n",
            &[
                ("blocks", "helper"),
                ("outer", "check"),
                ("outer", "blocks"),
            ][..],
        ),
        (
            "m.go",
            "package m\nfunc outer(src int) int {\n\thelper := func(x int) int { return inc(x) }\n\treturn blocks(helper(src))\n}\n",
            &[("outer", "inc"), ("outer", "blocks"), ("outer", "helper")][..],
        ),
        (
            "M.java",
            "class Outer {\n  int outer() {\n    class Inner { int run() { return helper(1); } }\n    Runnable r = () -> later();\n    return blocks(2);\n  }\n}\n",
            &[("run", "helper"), ("outer", "later"), ("outer", "blocks")][..],
        ),
        (
            "M.cs",
            "class Outer {\n  int Run() {\n    int Helper(int x) { return Inc(x); }\n    return Blocks(Helper(1));\n  }\n}\n",
            &[("Helper", "Inc"), ("Run", "Blocks"), ("Run", "Helper")][..],
        ),
        (
            "m.scala",
            "object O {\n  def outer(src: Int): Int = {\n    def helper(x: Int): Int = inc(x)\n    blocks(helper(src))\n  }\n}\n",
            &[("helper", "inc"), ("outer", "blocks"), ("outer", "helper")][..],
        ),
        (
            "m.cpp",
            "int outer(int src) {\n  auto helper = [](int x) { return inc(x); };\n  return blocks(helper(src));\n}\nstruct S { int run() { struct L { int go() { return deep(); } }; return top(); } };\n",
            &[
                ("outer", "inc"),
                ("outer", "blocks"),
                ("outer", "helper"),
                ("go", "deep"),
                ("run", "top"),
            ][..],
        ),
    ] {
        let got = callers(source, path);
        let want = pairs(expected);
        for pair in &want {
            assert!(got.contains(pair), "{path}: expected {pair:?} in {got:?}");
        }
        assert_eq!(got.len(), want.len(), "{path}: {got:?}");
    }
}
