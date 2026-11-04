//! Go facts: what an import binds, the type a method is declared on, the
//! types a struct or interface embeds, and generic calls.

use super::*;
use serde_json::json;

fn facts(source: &str) -> Value {
    facts_json(source, "a.go")
}

#[test]
fn go_imports_record_alias_dot_and_blank_bindings() {
    let facts = facts(
        "package a\n\nimport (\n\t\"example.com/app/plain\"\n\tst \"example.com/app/store\"\n\t. \"example.com/app/dot\"\n\t_ \"example.com/app/side\"\n)\n",
    );
    let bindings = facts["imports"]
        .as_array()
        .expect("imports")
        .iter()
        .map(|import| {
            (
                import["specifier"].as_str().unwrap_or_default().to_owned(),
                import["localName"].as_str().map(str::to_owned),
                import["importedName"].as_str().map(str::to_owned),
            )
        })
        .collect::<Vec<_>>();
    let binding = |spec: &str, local: Option<&str>, imported: Option<&str>| {
        (
            format!("example.com/app/{spec}"),
            local.map(str::to_owned),
            imported.map(str::to_owned),
        )
    };
    assert_eq!(
        bindings,
        [
            binding("plain", None, None),
            binding("store", Some("st"), None),
            binding("dot", Some("."), Some("*")),
            binding("side", Some("_"), None),
        ]
    );
}

#[test]
fn go_methods_record_their_receiver_base_type() {
    let facts = facts(
        "package a\n\ntype List[T any] struct{}\n\nfunc (s *Store) Get() {}\nfunc (Store) Value() {}\nfunc (l *List[T]) Push(v T) {}\nfunc Free() {}\n",
    );
    let owners = facts["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .filter(|declaration| declaration["kind"] != "type")
        .map(|declaration| {
            (
                declaration["name"].as_str().unwrap_or_default().to_owned(),
                declaration["owner"].as_str().map(str::to_owned),
            )
        })
        .collect::<Vec<_>>();
    let owned = |name: &str, owner: Option<&str>| (name.to_owned(), owner.map(str::to_owned));
    assert_eq!(
        owners,
        [
            owned("Get", Some("Store")),
            owned("Value", Some("Store")),
            owned("Push", Some("List")),
            owned("Free", None),
        ]
    );
}

/// Embedded fields (plain, pointer, qualified, generic) and embedded
/// interfaces; named fields and type-set terms embed nothing.
#[test]
fn go_embedded_fields_and_interfaces_are_embeds_edges() {
    let facts = facts(
        "package a\n\ntype S struct {\n\tbase\n\t*Store\n\tio.Writer\n\tList[int]\n\tname string\n}\n\ntype R interface {\n\tio.Reader\n\tGetter\n\tRun()\n}\n\ntype N interface {\n\t~int | string\n}\n",
    );
    let declarations = facts["declarations"].as_array().expect("declarations");
    let mut embeds = facts["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .filter(|edge| edge["relation"] == "embeds")
        .map(|edge| {
            let from = declarations
                .iter()
                .find(|declaration| declaration["id"] == edge["from"])
                .and_then(|declaration| declaration["name"].as_str())
                .unwrap_or_default();
            format!("{from}->{}", edge["to"].as_str().unwrap_or_default())
        })
        .collect::<Vec<_>>();
    embeds.sort();
    assert_eq!(
        embeds,
        [
            "R->Getter",
            "R->io.Reader",
            "S->List",
            "S->Store",
            "S->base",
            "S->io.Writer"
        ]
    );
}

/// `f[T](a, b)` calls `f`; `T[int](x)` is a conversion to the syntax tree,
/// even when `T` is a function.
#[test]
fn go_explicitly_instantiated_calls_name_the_generic_function() {
    let facts =
        facts("package a\n\nfunc u() {\n\tMap[int](a, b)\n\tlist.Map[int, string](a, b)\n}\n");
    let callees = facts["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .filter_map(|call| call["callee"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(callees, ["Map", "list.Map"]);
}

fn declared(facts: &Value) -> Vec<(String, String, bool)> {
    facts["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .map(|declaration| {
            (
                declaration["name"].as_str().unwrap_or_default().to_owned(),
                declaration["kind"].as_str().unwrap_or_default().to_owned(),
                declaration["exported"] == true,
            )
        })
        .collect()
}

#[test]
fn go_var_and_grouped_const_declare_every_name() {
    let facts = facts(
        "package a\n\nconst (\n\tCA = iota\n\tcb\n)\nconst One, two = 1, 2\nvar VA, vb = 1, 2\nvar (\n\tVC int\n\tvd string\n)\n",
    );
    let names = |kind: &str| {
        declared(&facts)
            .into_iter()
            .filter(|(_, declared_kind, _)| declared_kind == kind)
            .map(|(name, _, exported)| (name, exported))
            .collect::<Vec<_>>()
    };
    let expect = |rows: &[(&str, bool)]| {
        rows.iter()
            .map(|(name, exported)| ((*name).to_owned(), *exported))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names("constant"),
        expect(&[("CA", true), ("cb", false), ("One", true), ("two", false)])
    );
    assert_eq!(
        names("variable"),
        expect(&[("VA", true), ("vb", false), ("VC", true), ("vd", false)])
    );
}

/// A function's locals are not the package's: the declaration stays at the
/// spec, each name selecting itself.
#[test]
fn go_local_vars_stay_undeclared_and_each_name_selects_itself() {
    let facts =
        facts("package a\n\nfunc F() {\n\tvar local = 1\n\t_ = local\n}\n\nvar A, B = 1, 2\n");
    let names = declared(&facts)
        .into_iter()
        .map(|(name, _, _)| name)
        .collect::<Vec<_>>();
    assert_eq!(names, ["F", "A", "B"]);
    let selection = |name: &str| {
        facts["declarations"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["name"] == name))
            .map(|row| row["selectionRange"]["start"]["character"].clone())
    };
    assert_eq!(selection("A"), Some(json!(4)));
    assert_eq!(selection("B"), Some(json!(7)));
}

#[test]
fn go_facts_record_the_package_clause() {
    assert_eq!(
        facts("// Package doc.\npackage lib\n\nfunc F() {}\n")["goPackage"],
        "lib"
    );
    assert_eq!(facts("package lib\n")["buildExcluded"], Value::Null);
}

/// Only a constraint no build satisfies excludes the file: `ignore` alone or
/// beside a conjunction, in either spelling, above the package clause.
#[test]
fn go_build_ignore_leaves_no_graph_facts() {
    for header in [
        "//go:build ignore",
        "//go:build ignore && linux",
        "//go:build linux && ignore",
        "// +build ignore",
        "// Generator.\n//go:build ignore",
    ] {
        let source = format!(
            "{header}\n\npackage main\n\nimport \"example.com/app/x\"\n\nfunc main() {{ x.Run() }}\n"
        );
        let facts = facts(&source);
        assert_eq!(facts["buildExcluded"], true, "{header}");
        for field in ["imports", "calls", "declarations"] {
            assert_eq!(facts[field], json!([]), "{header}: {field}");
        }
        assert!(
            facts["diagnostics"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["code"] == "buildExcluded")),
            "{header}"
        );
    }
    for header in [
        "//go:build linux",
        "//go:build !ignore",
        "//go:build ignore || linux",
        "// A comment naming go:build ignore in prose.",
    ] {
        let source = format!("{header}\n\npackage main\n\nfunc main() {{}}\n");
        let facts = facts(&source);
        assert_eq!(facts["buildExcluded"], Value::Null, "{header}");
        assert_eq!(facts["declarations"][0]["name"], "main", "{header}");
    }
    // After the clause it is only a comment.
    let late = facts("package main\n\n//go:build ignore\n\nfunc main() {}\n");
    assert_eq!(late["buildExcluded"], Value::Null);
}

/// An outline and an enclosing-name lookup still read such a file.
#[test]
fn go_build_ignore_keeps_its_outline() {
    let document = crate::signatures::extract_declarations_document(
        "//go:build ignore\n\npackage main\n\nfunc main() {}\n",
        "a.go",
    )
    .expect("declarations");
    assert_eq!(document.declarations[0].name, "main");
}
