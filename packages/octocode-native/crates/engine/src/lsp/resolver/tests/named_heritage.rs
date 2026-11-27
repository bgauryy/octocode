//! The heritage a walk of only the nodes spelling a base finds is the heritage
//! of that base the whole file's graph facts record.
use crate::graph::GraphFactsDocument;
use crate::signatures::{extract_graph_facts, extract_named_heritage};

/// (declaration name, its name line, relation, base as written) of every edge
/// to `base`, in order.
fn heritage(document: &GraphFactsDocument, base: &str) -> Vec<(String, u32, String, String)> {
    let mut found = document
        .edges
        .iter()
        .filter(|edge| edge.to.rsplit(['.', ':']).next() == Some(base))
        .filter_map(|edge| {
            let declaration = document
                .declarations
                .iter()
                .find(|declaration| declaration.id == edge.from)?;
            Some((
                declaration.name.clone(),
                declaration.selection_range.start.line,
                edge.relation.clone(),
                edge.to.clone(),
            ))
        })
        .collect::<Vec<_>>();
    found.sort();
    found
}

fn same(file: &str, source: &str, base: &str, count: usize) {
    let whole = heritage(&extract_graph_facts(source, file).expect("facts"), base);
    let narrow = heritage(
        &extract_named_heritage(source, file, base).expect("narrowed"),
        base,
    );
    assert_eq!(narrow, whole, "{file}");
    assert_eq!(whole.len(), count, "{file}: {whole:?}");
}

#[test]
fn every_grammar_records_the_heritage_its_facts_do() {
    same(
        "A.java",
        "class A extends Base implements Querier {}\ninterface Q extends Querier {}\nclass B { void f() { Querier q = null; } }\n",
        "Querier",
        2,
    );
    same(
        "a.py",
        "class A(Querier):\n    pass\nclass B(object):\n    q = Querier\n",
        "Querier",
        1,
    );
    same(
        "a.rs",
        "trait Querier {}\nstruct S;\nimpl Querier for S {}\ntrait Sub: Querier + Send {}\nfn f(q: &dyn Querier) {}\n",
        "Querier",
        2,
    );
    same(
        "a.go",
        "package p\ntype Q interface {\n\tQuerier\n\tClose()\n}\ntype S struct{ Querier }\nfunc f(q Querier) {}\n",
        "Querier",
        2,
    );
}

/// oxc owns these files: the whole facts answer.
#[test]
fn oxc_files_are_left_to_the_whole_facts() {
    assert!(extract_named_heritage("class A extends Base {}\n", "a.ts", "Base").is_none());
}
