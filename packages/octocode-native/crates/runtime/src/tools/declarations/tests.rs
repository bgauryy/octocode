use super::*;

#[test]
fn each_request_owns_its_declaration_document() {
    let source = "/// Doc.\nfn first() {\n    1;\n}\nfn second() {}\n";
    let first = file_document(source, "a.rs").expect("first");
    let second = file_document(source, "a.rs").expect("second");
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(first, second);
    assert_eq!(
        *first,
        octocode_engine::signatures::extract_declarations_document(source, "a.rs").expect("engine")
    );
    let declared = all_declarations(source, "a.rs").expect("declarations");
    assert_eq!(
        declared
            .iter()
            .map(|d| (d.name.as_str(), d.doc_start, d.line, d.start, d.end))
            .collect::<Vec<_>>(),
        [("first", Some(1), 2, 2, 4), ("second", None, 5, 5, 5)]
    );
}

#[test]
fn explicit_grammar_and_source_changes_are_read_directly() {
    let source = "export function hello(value: number): number { return value; }";
    assert_eq!(
        file_document(source, "script").as_deref(),
        octocode_engine::signatures::extract_declarations_document(source, "script").as_ref(),
        "extensionless behavior belongs to the grammar owner"
    );
    let parsed = document(source, "script", Some("ts")).expect("override");
    assert!(parsed.declarations.iter().any(|d| d.name == "hello"));
    let edited = document("export function changed() {}", "script", Some("ts")).expect("edit");
    assert!(edited.declarations.iter().any(|d| d.name == "changed"));
    assert!(!edited.declarations.iter().any(|d| d.name == "hello"));
}

#[test]
fn full_facts_preserve_heritage_and_reflect_edits() {
    let source = "export class A extends B {}\n";
    let first = graph_facts(source, "a.ts").expect("facts");
    let again = graph_facts(source, "a.ts").expect("facts");
    assert_eq!(first, again);
    assert!(!Arc::ptr_eq(&first, &again));
    assert!(first.edges.iter().any(|edge| edge.relation == "extends"));
    let edited = graph_facts("export class A {}\n", "a.ts").expect("edited facts");
    assert!(!edited.edges.iter().any(|edge| edge.relation == "extends"));
}

#[test]
fn concurrent_requests_have_equal_independent_documents() {
    let gate = std::sync::Barrier::new(4);
    let documents = std::thread::scope(|scope| {
        let workers = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    gate.wait();
                    graph_facts("export class Shared extends Base {}", "a.ts").expect("facts")
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("worker"))
            .collect::<Vec<_>>()
    });
    for document in &documents[1..] {
        assert_eq!(document, &documents[0]);
        assert!(!Arc::ptr_eq(document, &documents[0]));
    }
}
