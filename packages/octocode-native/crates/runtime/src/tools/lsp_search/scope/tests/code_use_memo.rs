//! A repeat over an unchanged file keeps its answer; a changed file answers
//! anew.
use super::super::code_use_memo::reads;
use super::*;
use octocode_engine::stamp::settle_written_so_far;

/// The text-only files of one fresh request for `name` declared in `a.rs`.
fn text_only(root: &Path, name: &str) -> Vec<String> {
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.rs".into()]);
    scope.asks(Question::Uses);
    let policy = workspace_policy(root);
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(scope.text_files(name, &policy, &crate::execution::cancel::NeverCancel))
        .expect("scan");
    scope.answer([root.join("a.rs").to_string_lossy()]);
    scope.text_only_files(name).expect("known")
}

fn files(root: &Path) -> (PathBuf, String) {
    write(&root.join("a.rs"), "pub fn origin() {}\n");
    let other = root.join("b.rs");
    (other.clone(), other.to_string_lossy().into_owned())
}

#[test]
fn a_repeat_over_a_settled_file_reads_it_once() {
    let (_dir, root) = fixture();
    let (other, name) = files(&root);
    write(&other, "fn f() { origin(); }\n");
    settle_written_so_far();
    assert_eq!(text_only(&root, "origin"), std::slice::from_ref(&name));
    assert_eq!(text_only(&root, "origin"), [name]);
    assert_eq!(reads::of(&other), 1);
}

/// Another name and another question have their own answers.
#[test]
fn the_answer_is_kept_per_name_and_question() {
    let (_dir, root) = fixture();
    let (other, name) = files(&root);
    write(&other, "pub fn origin() {}\nfn f() { helper(); }\n");
    write(
        &root.join("a.rs"),
        "pub fn origin() {}\npub fn helper() {}\n",
    );
    settle_written_so_far();
    assert_eq!(text_only(&root, "origin"), Vec::<String>::new());
    assert_eq!(text_only(&root, "helper"), std::slice::from_ref(&name));
    assert_eq!(reads::of(&other), 2);

    let spelling = Scope::new(root.to_string_lossy().into_owned(), vec!["*.rs".into()]);
    let policy = workspace_policy(&root);
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(spelling.text_files("origin", &policy, &crate::execution::cancel::NeverCancel))
        .expect("scan");
    spelling.answer([root.join("a.rs").to_string_lossy()]);
    assert_eq!(spelling.text_only_files("origin"), Some(vec![name]));
    assert_eq!(reads::of(&other), 3);
}

/// Same length, so only the file's times and the proof they give can tell.
#[test]
fn a_changed_file_answers_anew_and_is_not_served_stale() {
    let (_dir, root) = fixture();
    let (other, name) = files(&root);
    write(&other, "fn f() { origin(); }\n");
    settle_written_so_far();
    assert_eq!(text_only(&root, "origin"), std::slice::from_ref(&name));

    write(&other, "fn f() { /*origin*/}\n");
    settle_written_so_far();
    assert_eq!(text_only(&root, "origin"), Vec::<String>::new());
    assert_eq!(reads::of(&other), 2);

    write(&other, "fn f() { origin(); }\n");
    settle_written_so_far();
    assert_eq!(text_only(&root, "origin"), [name]);
    assert_eq!(reads::of(&other), 3);
}

/// A write inside the settle window could leave the times as they were, so
/// the answer from such a file is not kept.
#[test]
fn an_unsettled_file_is_read_every_time() {
    let (_dir, root) = fixture();
    let (other, name) = files(&root);
    write(&other, "fn f() { origin(); }\n");
    assert_eq!(text_only(&root, "origin"), std::slice::from_ref(&name));
    assert_eq!(text_only(&root, "origin"), [name]);
    assert_eq!(reads::of(&other), 2);
}

/// A file gone is unreadable: a use by the rule for one, with nothing kept
/// that a later file at the path could be served from.
#[test]
fn an_unreadable_file_counts_as_a_use_and_is_not_kept() {
    let (_dir, root) = fixture();
    let (other, _) = files(&root);
    let file = other.to_string_lossy().into_owned();
    assert_eq!(
        super::super::code_use_memo::kept_or_read("origin", &file, Question::Uses, || None),
        None
    );
    write(&other, "// origin\n");
    settle_written_so_far();
    assert_eq!(
        super::super::code_use_memo::kept_or_read("origin", &file, Question::Uses, || Some(false)),
        Some(false)
    );
}
