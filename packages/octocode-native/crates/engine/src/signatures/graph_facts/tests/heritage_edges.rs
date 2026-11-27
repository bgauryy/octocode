//! `extends`/`implements` edges per language.

use super::*;

#[test]
fn rust_heritage_links_impl_trait_and_supertraits() {
    let source = "trait Shape: Clone + std::fmt::Debug + 'static {}\nstruct Square;\nimpl std::fmt::Display for Square {}\nimpl<T> From<T> for Square {}\nimpl Square {}\n";
    assert_eq!(
        heritage(source, "lib.rs"),
        vec![
            edge("extends", "Shape", "Clone", 1),
            edge("extends", "Shape", "std::fmt::Debug", 1),
            edge("implements", "Square", "std::fmt::Display", 3),
            edge("implements", "Square", "From", 4),
        ]
    );
    let facts = facts_json(source, "lib.rs");
    let impl_ids: Vec<&Value> = facts["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .filter(|declaration| declaration["kind"] == "impl")
        .map(|declaration| &declaration["id"])
        .collect();
    let froms: Vec<&Value> = facts["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .filter(|edge| edge["relation"] == "implements")
        .map(|edge| &edge["from"])
        .collect();
    assert_eq!(
        froms,
        impl_ids[..2].to_vec(),
        "from is the impl declaration id"
    );
}

/// A raw identifier (`r#Trait`) names a type like any other: an impl or a
/// supertrait bound spelled raw links to it, under the spelling written.
#[test]
fn rust_heritage_links_raw_identifiers() {
    let source =
        "trait r#Shape: r#Base {}\nstruct S;\nimpl r#Trait for S {}\nimpl a::r#Trait<T> for S {}\n";
    assert_eq!(
        heritage(source, "lib.rs"),
        vec![
            edge("extends", "r#Shape", "r#Base", 1),
            edge("implements", "S", "r#Trait", 3),
            edge("implements", "S", "a::r#Trait", 4),
        ]
    );
}

#[test]
fn python_heritage_skips_keywords_splats_and_object() {
    let source = "class A(Base, pkg.Mixin, Generic[T], metaclass=Meta):\n    pass\nclass B(object):\n    pass\nclass C(*bases, **kw):\n    pass\n";
    assert_eq!(
        heritage(source, "m.py"),
        vec![
            edge("extends", "A", "Base", 1),
            edge("extends", "A", "pkg.Mixin", 1),
            edge("extends", "A", "Generic", 1),
        ]
    );
}

#[test]
fn java_heritage_separates_extends_and_implements() {
    let source = "class A extends Base<String> implements Runnable, java.io.Serializable {}\ninterface I extends J, K<T> {}\nenum E implements I {}\nrecord R(int x) implements I {}\n";
    assert_eq!(
        heritage(source, "A.java"),
        vec![
            edge("extends", "A", "Base", 1),
            edge("implements", "A", "Runnable", 1),
            edge("implements", "A", "java.io.Serializable", 1),
            edge("extends", "I", "J", 2),
            edge("extends", "I", "K", 2),
            edge("implements", "E", "I", 3),
            edge("implements", "R", "I", 4),
        ]
    );
}

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn cpp_heritage_reads_the_base_class_clause() {
    let source = "class A : public Base, private ns::Mixin<int> {};\nstruct S : virtual Base {};\nclass Plain {};\n";
    assert_eq!(
        heritage(source, "a.cpp"),
        vec![
            edge("extends", "A", "Base", 1),
            edge("extends", "A", "ns::Mixin", 1),
            edge("extends", "S", "Base", 2),
        ]
    );
}

#[cfg(feature = "tree-sitter-c-sharp")]
#[test]
fn csharp_heritage_treats_the_first_class_base_as_extends() {
    let source = "class A : Base, IDisposable, IList<int> {}\ninterface I : J, K {}\nstruct S : IEquatable<S> {}\nrecord R(int X) : Base(X);\n";
    assert_eq!(
        heritage(source, "A.cs"),
        vec![
            edge("extends", "A", "Base", 1),
            edge("implements", "A", "IDisposable", 1),
            edge("implements", "A", "IList", 1),
            edge("extends", "I", "J", 2),
            edge("extends", "I", "K", 2),
            edge("implements", "S", "IEquatable", 3),
            edge("extends", "R", "Base", 4),
        ]
    );
}

#[test]
fn heritage_edges_name_their_unresolved_base() {
    let extraction =
        extract_graph_facts_with_metadata("class A(Base):\n    pass\n", "m.py").expect("facts");
    let extends = extraction
        .facts
        .edges
        .iter()
        .find(|edge| edge.relation == "extends")
        .expect("extends edge");
    let class = &extraction.facts.declarations[0];
    assert_eq!(class.name, "A");
    assert_eq!(extends.from, class.id);
    assert_eq!(extends.to, "Base");
}

/// The clause dispatch sends a grammar to the C++-style base clause only
/// when it is not one of the named forms; no named form's grammar has a
/// class or struct specifier node, so no node of theirs is read twice.
#[test]
fn named_heritage_forms_have_no_base_class_specifiers() {
    for entry in crate::signatures::languages::all_entries() {
        let clauses = super::super::heritage::Clauses::of(entry.extensions[0]);
        if clauses == super::super::heritage::Clauses::BaseClass {
            continue;
        }
        for kind in ["class_specifier", "struct_specifier"] {
            assert_eq!(
                entry.language.id_for_node_kind(kind, true),
                0,
                "{} ({clauses:?}) has a {kind} node",
                entry.name
            );
        }
    }
}

/// Go records embedded types as `embeds` and never names a supertype, so a
/// subtype scan can skip its files; every other heritage form may.
#[test]
fn only_go_heritage_never_names_a_supertype() {
    let go = "package a\n\ntype S struct {\n\tQuerier\n}\n\ntype R interface {\n\tQuerier\n}\n";
    assert!(heritage(go, "a.go").is_empty());
    assert!(
        facts_json(go, "a.go")["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .any(|edge| edge["relation"] == "embeds")
    );
    assert!(!crate::signatures::may_name_supertypes(go, "a.go"));
    for (source, path) in [
        ("trait A: B {}\n", "lib.rs"),
        ("class A(B):\n    pass\n", "m.py"),
        ("class A extends B {}\n", "A.java"),
        ("class A : B {}\n", "A.cs"),
        ("class A : public B {};\n", "a.cpp"),
        ("class A extends B\n", "a.scala"),
        ("class A extends B {}\n", "a.ts"),
        ("plain text\n", "notes.txt"),
    ] {
        assert!(
            crate::signatures::may_name_supertypes(source, path),
            "{path}"
        );
    }
}
