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
