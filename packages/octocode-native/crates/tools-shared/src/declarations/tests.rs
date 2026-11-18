use super::*;

#[test]
fn an_unsupported_grammar_still_reuses_its_negative_result() {
    let source = "not a supported source file";
    let path = format!(
        "/store-unsupported-{}/source.unsupported",
        std::process::id()
    );
    assert!(graph_facts(source, &path).is_none());
    assert!(graph_facts(source, &path).is_none());
    assert_eq!(parses::facts_of(&path), 1);
}

#[test]
fn a_cancelled_parse_does_not_poison_the_next_live_request() {
    let source = "export class Left implements Base {}";
    let path = format!("/store-cancelled-{}/left.ts", std::process::id());
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let cancelled =
        octocode_engine::deep_stack::while_stoppable(&stopped, || graph_facts(source, &path));
    assert!(cancelled.is_none(), "a stopped extraction has no document");
    let live = graph_facts(source, &path).expect("the next live request parses the source");
    assert_eq!(
        *live,
        octocode_engine::signatures::extract_graph_facts(source, &path).expect("engine")
    );
    assert!(
        live.declarations
            .iter()
            .any(|declaration| declaration.name == "Left")
    );
    let kept = graph_facts(source, &path).expect("the live document is reusable");
    assert!(Arc::ptr_eq(&live, &kept));
    assert_eq!(parses::facts_of(&path), 2);
}

#[test]
fn a_text_read_again_is_its_kept_document() {
    let source = "/// Doc.\nfn first() {\n    1;\n}\nfn second() {}\n";
    let path = format!("/store-kept-{}/a.rs", std::process::id());
    let first = file_document(source, &path).expect("first");
    let second = file_document(source, &path).expect("second");
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(parses::of("declarations", &path), 1);
    assert_eq!(
        *first,
        octocode_engine::signatures::extract_declarations_document(source, &path).expect("engine")
    );
    // Each analysis, path and text is its own document.
    let facts = graph_facts(source, &path).expect("facts");
    assert!(!Arc::ptr_eq(&first, &facts));
    assert_eq!(parses::facts_of(&path), 1);
    let elsewhere = format!("/store-kept-{}/b.rs", std::process::id());
    let moved = file_document(source, &elsewhere).expect("other path");
    assert_eq!(moved.file, elsewhere);
    assert_eq!(first.file, path);
    let edited = file_document("fn third() {}\n", &path).expect("edited");
    assert_eq!(edited.declarations[0].name, "third");
    assert_eq!(parses::of("declarations", &path), 2);
    let again = file_document(source, &path).expect("again");
    assert!(
        Arc::ptr_eq(&first, &again),
        "an edit keeps the old text's document"
    );
}

#[test]
fn each_declaration_document_matches_the_engine() {
    let source = "/// Doc.\nfn first() {\n    1;\n}\nfn second() {}\n";
    let first = file_document(source, "a.rs").expect("first");
    let second = file_document(source, "a.rs").expect("second");
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
    assert!(first.edges.iter().any(|edge| edge.relation == "extends"));
    let edited = graph_facts("export class A {}\n", "a.ts").expect("edited facts");
    assert!(!edited.edges.iter().any(|edge| edge.relation == "extends"));
}

#[test]
fn concurrent_requests_have_equal_documents() {
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
    }
}
