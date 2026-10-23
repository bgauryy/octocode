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
