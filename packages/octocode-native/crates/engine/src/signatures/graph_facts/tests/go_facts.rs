//! Go facts: what an import binds, the type a method is declared on, the
//! types a struct or interface embeds, and generic calls.

use super::*;

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
