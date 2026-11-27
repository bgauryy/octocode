//! Canonical repository URLs from registry link forms.
use super::{canonical_repository, github_slug};

/// Every registry link form normalizes to one canonical URL, and that
/// URL names the same owner/repo the GitHub tools take.
#[test]
fn every_repository_link_form_is_one_canonical_url() {
    for (input, expected) in [
        (
            "github:octokit/rest.js",
            Some("https://github.com/octokit/rest.js"),
        ),
        (
            "git+https://github.com/a/b.git",
            Some("https://github.com/a/b"),
        ),
        ("git@github.com:a/b.git", Some("https://github.com/a/b")),
        (
            "ssh://git@github.com/a/b.git",
            Some("https://github.com/a/b"),
        ),
        (
            "ssh://git@github.com:22/a/b",
            Some("https://github.com/a/b"),
        ),
        ("git://github.com/a/b.git", Some("https://github.com/a/b")),
        (
            "scm:git:git@github.com:a/b.git",
            Some("https://github.com/a/b"),
        ),
        (
            "scm:git:https://github.com/a/b",
            Some("https://github.com/a/b"),
        ),
        ("https://github.com/a/b/", Some("https://github.com/a/b")),
        (
            "https://github.com/a/b.git/",
            Some("https://github.com/a/b"),
        ),
        ("a/b", Some("https://github.com/a/b")),
        ("github.com/a/b", Some("https://github.com/a/b")),
        (
            "https://github.com/rails/rails/tree/v8.1.4",
            Some("https://github.com/rails/rails/tree/v8.1.4"),
        ),
        ("https://gitlab.com/g/x", Some("https://gitlab.com/g/x")),
        ("https://user:secret@github.com/a/b", None),
        ("ftp://github.com/a/b", None),
        ("", None),
        ("not a link", None),
    ] {
        assert_eq!(canonical_repository(input).as_deref(), expected, "{input}");
        if let Some(expected) = expected.filter(|url| url.contains("github.com/a/b")) {
            assert_eq!(
                github_slug(expected),
                Some(("a".to_owned(), "b".to_owned()))
            );
        }
    }
}

/// A source link is its repository and, for a GitHub page below a ref, the
/// path segments under `tree`/`blob`: the link alone does not say how many of
/// them are the ref, so none is called a directory here.
#[test]
fn a_source_link_splits_into_the_repository_and_its_page_path() {
    use super::source_link;
    let split = |link| source_link(link).map(|found| (found.repository, found.ref_path));
    let segments = |parts: &[&str]| Some(parts.iter().map(|part| (*part).to_owned()).collect());
    assert_eq!(
        split("git+https://github.com/o/r/tree/main/pkgs/a.git"),
        Some((
            "https://github.com/o/r".to_owned(),
            segments(&["main", "pkgs", "a"])
        ))
    );
    assert_eq!(
        split("https://github.com/o/r/blob/users/x/pkg/a.rs"),
        Some((
            "https://github.com/o/r".to_owned(),
            segments(&["users", "x", "pkg"])
        ))
    );
    for page in [
        "https://github.com/o/r/issues",
        "https://github.com/o/r/tree/main",
        "https://github.com/o/r/blob/main/a.rs",
    ] {
        assert_eq!(
            split(page),
            Some(("https://github.com/o/r".to_owned(), None)),
            "{page}"
        );
    }
    assert_eq!(
        split("https://gitlab.com/o/r"),
        Some(("https://gitlab.com/o/r".to_owned(), None))
    );
    assert_eq!(split("not a link"), None);
}
