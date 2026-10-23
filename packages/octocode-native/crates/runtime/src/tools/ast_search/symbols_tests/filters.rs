//! What a directory scan holds under name and kind filters, and what an
//! empty name search blames.

use super::*;

/// A `languageGlobs` refusal keeps its declared code in astSearch: it reads
/// the engine's typed scan error.
#[test]
fn a_bad_language_glob_is_refused_with_its_declared_code() {
    let root = Fixture::new();
    files(&root.0, 1);
    let (paths, _) = simple_policy(&root.0);
    let canonical = &root.0.canonicalize().expect("root");
    for (globs, code) in [
        (json!({"cpp": ["["]}), ErrorCode::InvalidGlob),
        (
            json!({"no-such-grammar": ["*.x"]}),
            ErrorCode::LanguageUnsupported,
        ),
    ] {
        let q = query(&root.0, json!({"languageGlobs": globs}));
        let Err(error) = outline_directory(&q, canonical.as_path(), &paths, &NeverCancel) else {
            panic!("{code}: the scan must be refused");
        };
        assert_eq!(error.code, code, "{}", error.message);
    }
}

/// Only declarations whose name can match `symbolName` leave the scan; every
/// parsed file is still counted as scanned.
#[test]
fn a_name_search_retains_only_matching_declarations() {
    let root = Fixture::new();
    files(&root.0, 30);
    let held = scan(&query(&root.0, json!({"symbolName":"Disposable"})), &root.0);
    assert_eq!(rows(&held), 1, "rows held while scanning");
    assert_eq!(held.files.len(), 1, "files held while scanning");
    assert_eq!(held.scanned, 30, "every parsed file is counted");
}

/// A `kinds` filter drops other kinds before they are held.
#[test]
fn a_kind_search_retains_only_the_asked_kinds() {
    let root = Fixture::new();
    files(&root.0, 30);
    let held = scan(&query(&root.0, json!({"kinds":["class"]})), &root.0);
    assert_eq!(rows(&held), 31, "30 Widget classes and Disposable");
    assert_eq!(held.scanned, 30);
}

/// With a name, a declaration of another kind is held only when its name
/// equals an entry (it keeps that entry exact); one that merely contains the
/// text is dropped like any other kind.
#[test]
fn a_kind_and_name_search_holds_only_equal_names_of_other_kinds() {
    let root = Fixture::new();
    files(&root.0, 30);
    let off_kind =
        |outline: &Outline| -> usize { outline.files.iter().map(|file| file.off_kind.len()).sum() };
    let exact = scan(
        &query(
            &root.0,
            json!({"kinds":["function"],"symbolName":"Disposable"}),
        ),
        &root.0,
    );
    assert_eq!((rows(&exact), off_kind(&exact)), (0, 1));
    let substring = scan(
        &query(&root.0, json!({"kinds":["function"],"symbolName":"Dispos"})),
        &root.0,
    );
    assert_eq!((rows(&substring), off_kind(&substring)), (0, 0));
    assert!(substring.files.is_empty(), "no file contributes");
}

/// An empty outline whose files the walk reached claims nothing hides behind
/// skipped paths: the scanned files are not "behind" anything.
#[test]
fn an_empty_name_search_does_not_blame_files_it_scanned() {
    let root = Fixture::new();
    files(&root.0, 5);
    let out = empty_outline(&root.0, json!({"symbolName":"zzNoSuchSymbol"}));
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(out["filesScanned"], 5, "{out}");
    checks::assert_absent(&out["hints"], "skipped paths");
    assert!(out["next"].get("includeSkipped").is_none(), "{out}");
}

/// Files behind a hidden directory are counted by themselves, not with the
/// files the scan reached.
#[test]
fn an_empty_name_search_counts_only_the_hidden_files() {
    let root = Fixture::new();
    files(&root.0, 5);
    std::fs::create_dir_all(root.0.join(".cfg")).expect("dir");
    root.write(".cfg/hidden.ts", "export const zzNeedle = 1;\n");
    let out = empty_outline(&root.0, json!({"symbolName":"zzNeedle"}));
    assert_eq!(out["status"], "empty", "{out}");
    let hints = out["hints"].to_string();
    assert!(hints.contains("1 outlinable file"), "{hints}");
    assert!(hints.contains("hidden .cfg/"), "{hints}");
}

/// A trait's required method (no body) is a declaration a name search
/// finds, not a false "0 declarations".
#[test]
fn a_name_search_finds_a_body_less_trait_method() {
    let root = Fixture::new();
    root.write(
        "judge.rs",
        "pub trait Judge {\n    fn identity(&self) -> u8;\n    fn project(&self) {}\n}\n",
    );
    let out = empty_outline(&root.0, json!({"symbolName":"identity"}));
    assert_ne!(out["status"], "empty", "{out}");
    assert!(
        out.to_string().contains("\"symbolName\":\"identity\""),
        "{out}"
    );
}
