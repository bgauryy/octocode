use std::collections::BTreeMap;

/// `method name -> receiverType` of every call in `source`. Fixture
/// methods under test are unique, so each lookup names exactly one call.
fn receivers(source: &str, path: &str) -> BTreeMap<String, Vec<Option<String>>> {
    let json = crate::signatures::graph_facts::tests::extract_graph_facts(source, path)
        .expect("graph facts");
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut out: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
    for call in value["calls"].as_array().unwrap() {
        let callee = call["callee"].as_str().unwrap();
        let callee = callee.split("::<").next().unwrap();
        let method = callee.rsplit(['.', ':', '>']).next().unwrap().to_owned();
        let receiver = call["receiverType"].as_str().map(str::to_owned);
        out.entry(method).or_default().push(receiver);
    }
    out
}

fn assert_receivers(source: &str, path: &str, expected: &[(&str, Option<&str>)]) {
    let actual = receivers(source, path);
    for (method, receiver) in expected {
        let found = actual
            .get(*method)
            .map(|items| items.iter().map(Option::as_deref).collect::<Vec<_>>());
        assert_eq!(
            found,
            Some(vec![*receiver]),
            "{path}: receiverType of `{method}` in {actual:?}"
        );
    }
}

#[test]
fn type_text_is_stripped_to_the_named_type() {
    use super::{Lang, clean_type_text};
    let clean = |text: &str, lang: Lang| clean_type_text(text, lang);
    assert_eq!(
        clean("&mut Store<'a, T>", Lang::Rust).as_deref(),
        Some("Store")
    );
    assert_eq!(
        clean("&'static mut Store", Lang::Rust).as_deref(),
        Some("Store")
    );
    assert_eq!(
        clean("Arc<Mutex<Store>>", Lang::Rust).as_deref(),
        Some("Mutex")
    );
    assert_eq!(
        clean("Box<dyn Sink + Send>", Lang::Rust).as_deref(),
        Some("Sink")
    );
    assert_eq!(clean("Vec::<u8>", Lang::Rust).as_deref(), Some("Vec"));
    assert_eq!(clean("*pkg.Store", Lang::Go).as_deref(), Some("pkg.Store"));
    assert_eq!(clean("Set[T]", Lang::Go).as_deref(), Some("Set"));
    assert_eq!(clean("[]Item", Lang::Go), None);
    assert_eq!(clean("map[string]Item", Lang::Go), None);
    assert_eq!(clean("Store[]", Lang::Java), None);
    assert_eq!(clean("int?", Lang::CSharp).as_deref(), Some("int"));
    assert_eq!(
        clean("struct ns::Store", Lang::Cpp).as_deref(),
        Some("ns::Store")
    );
    assert_eq!(clean("(A, B)", Lang::Rust), None);
    assert_eq!(clean("impl Fn()", Lang::Rust), None);
    assert_eq!(
        super::python_type_text("Optional[Store]").as_deref(),
        Some("Store")
    );
    assert_eq!(
        super::python_type_text("'models.Store'").as_deref(),
        Some("models.Store")
    );
    assert_eq!(
        super::python_type_text("Store | None").as_deref(),
        Some("Store")
    );
    assert_eq!(
        super::python_type_text("list[Store]").as_deref(),
        Some("list")
    );
    assert_eq!(super::python_type_text("A | B"), None);
}

mod csharp_cpp;
mod java_go;
mod rust_python;
