//! GitHub owner/repo slugs from registry links.
use super::github_slug;

#[test]
fn github_slugs_come_only_from_repository_links() {
    let slug = |owner: &str, repo: &str| Some((owner.to_owned(), repo.to_owned()));
    for (link, expected) in [
        ("https://github.com/o/r", slug("o", "r")),
        ("git+https://github.com/o/r.git", slug("o", "r")),
        ("git@github.com:o/r.git", slug("o", "r")),
        ("ssh://git@github.com/o/r.git", slug("o", "r")),
        (
            "https://github.com/o/r/tree/main/packages/x",
            slug("o", "r"),
        ),
        ("https://github.com/o/r.git/tree/main", slug("o", "r")),
        ("github.com/o/r#readme", slug("o", "r")),
        ("https://www.GitHub.com/o/r?tab=readme", slug("o", "r")),
        (
            "https://github.com/Textualize/rich/blob/master/CHANGELOG.md",
            slug("Textualize", "rich"),
        ),
        ("https://github.com/sponsors/hynek", None),
        ("https://github.com/orgs/o/r", None),
        ("https://github.com/hynek", None),
        ("https://github.com/o", None),
        ("https://gitlab.com/o/r", None),
        ("https://gist.github.com/o/r", None),
        ("https://notgithub.com/o/r", None),
    ] {
        assert_eq!(github_slug(link), expected, "{link}");
    }
}
