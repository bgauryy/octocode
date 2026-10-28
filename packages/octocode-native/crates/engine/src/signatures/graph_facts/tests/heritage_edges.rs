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
