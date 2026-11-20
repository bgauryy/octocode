//! What a question makes of a file's code uses, read from the parse that found
//! them: no whole-file facts parse per unanswered file.
use super::*;
use octocode_tools_shared::declarations::parses;

fn asks(question: Question, name: &str, source: &str) -> (bool, usize) {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("a.go");
    std::fs::write(&file, source).expect("write");
    let path = file.to_string_lossy().into_owned();
    let occurrences =
        octocode_engine::lsp::resolver::code_occurrences(&path, source, name).expect("occurrences");
    let answer = question.may_answer(name, &path, source, &occurrences);
    (answer, parses::facts_of(&path))
}

/// A type named in code may be an implementation's site; only calling the name
/// cannot. Either way the occurrences' parse answers.
#[test]
fn an_implementations_question_reads_the_occurrences_parse() {
    assert_eq!(
        asks(
            Question::Implementations,
            "Querier",
            "package p\nfunc use(q Querier) {\n\tq.Querier()\n}\n"
        ),
        (true, 0)
    );
    assert_eq!(
        asks(
            Question::Implementations,
            "Querier",
            "package p\nfunc use(q X) {\n\tq.Querier(\n\t\t1,\n\t)\n}\n"
        ),
        (false, 0)
    );
}

/// A uses question already does: declaring the name is no use.
#[test]
fn a_uses_question_reads_the_occurrences_parse() {
    assert_eq!(
        asks(Question::Uses, "origin", "package p\nfunc origin() {}\n"),
        (false, 0)
    );
    assert_eq!(
        asks(
            Question::Uses,
            "origin",
            "package p\nfunc origin() {}\nfunc f() { origin() }\n"
        ),
        (true, 0)
    );
}

/// A Rust item declared outside any type is asked about as such: no receiver
/// reaches a free function, so a member call of the name is no use of it,
/// while a path call is; a method keeps the plain uses question.
#[test]
fn a_free_items_uses_exclude_member_access() {
    let source = "pub fn write_atomic() {}\nimpl Held { pub fn write_atomic(&self) {} }\n";
    let path = "/w/lib.rs";
    let free = Question::Uses.at_anchor(path, Some(source), (0, 7), "write_atomic");
    assert_eq!(free, Question::ItemUses);
    let method = Question::Uses.at_anchor(path, Some(source), (1, 19), "write_atomic");
    assert_eq!(method, Question::Uses);
    assert_eq!(
        Question::Implementations.at_anchor(path, Some(source), (0, 7), "write_atomic"),
        Question::Implementations
    );
    let answers = |source: &str| {
        let occurrences =
            octocode_engine::lsp::resolver::code_occurrences("/w/b.rs", source, "write_atomic")
                .expect("occurrences");
        Question::ItemUses.may_answer("write_atomic", "/w/b.rs", source, &occurrences)
    };
    assert!(!answers(
        "fn f() { held.write_atomic(1); held\n    .write_atomic(2); }\n"
    ));
    assert!(answers(
        "fn f() { held.write_atomic(1); crate::write_atomic(); }\n"
    ));
    assert!(answers(
        "fn f() { let w = write_atomic; (0..1).map(|_| w()); }\n"
    ));
}

/// A Rust trait's implementations are its `impl … for` blocks and the
/// traits that extend it: a file that only takes `&dyn Trait`, bounds a
/// parameter by it or imports it holds none; one whose name a macro body
/// holds may, as only the whole file can tell.
#[test]
fn a_rust_types_implementations_are_its_heritage_sites() {
    let source = "pub trait CancellationCheck {}\n";
    assert_eq!(
        Question::Implementations.at_anchor(
            "/w/cancel.rs",
            Some(source),
            (0, 10),
            "CancellationCheck"
        ),
        Question::Heritage
    );
    // An anchor at an `impl Trait for Type` header names the trait too; one
    // at a `&dyn Trait` use stays the plain question.
    let header =
        "struct Never;\nimpl CancellationCheck for Never {}\nfn f(c: &dyn CancellationCheck) {}\n";
    let at = |line: u32, character: u32| {
        Question::Implementations.at_anchor(
            "/w/never.rs",
            Some(header),
            (line, character),
            "CancellationCheck",
        )
    };
    assert_eq!(at(1, 5), Question::Heritage);
    assert_eq!(at(2, 13), Question::Implementations);
    let answers = |source: &str| {
        let occurrences = octocode_engine::lsp::resolver::code_occurrences(
            "/w/b.rs",
            source,
            "CancellationCheck",
        )
        .expect("occurrences");
        Question::Heritage.may_answer("CancellationCheck", "/w/b.rs", source, &occurrences)
    };
    assert!(!answers(
        "use crate::CancellationCheck;\nfn f(c: &dyn CancellationCheck) {}\nfn g<T: CancellationCheck>(t: T) {}\n"
    ));
    assert!(answers(
        "struct Never;\nimpl crate::CancellationCheck for Never {}\n"
    ));
    assert!(answers("pub trait Deadline: CancellationCheck {}\n"));
    assert!(answers(
        "macro_rules! never {\n    ($t:ty) => { impl CancellationCheck for $t {} };\n}\n"
    ));
}
