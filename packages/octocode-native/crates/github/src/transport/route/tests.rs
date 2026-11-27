use super::*;

fn url(text: &str) -> Url {
    Url::parse(text).expect("url")
}

#[test]
fn a_followed_rename_routes_the_same_resource_under_the_id() {
    let from = url("https://ghe.example/api/v3/repos/Old/Name/contents/a%20b.md?ref=main");
    let to = url("https://ghe.example/api/v3/repositories/42/contents/a%20b.md?ref=main");
    let (old, new) = rename_route(&from, &to).expect("route");
    assert_eq!(old.as_str(), "https://ghe.example/api/v3/repos/old/name");
    assert_eq!(new.as_str(), "https://ghe.example/api/v3/repositories/42");
    let later = url("https://ghe.example/api/v3/repos/old/NAME/git/trees/x?recursive=1");
    assert_eq!(
        routed(&later, &new).expect("routed").as_str(),
        "https://ghe.example/api/v3/repositories/42/git/trees/x?recursive=1"
    );
    let metadata = url("https://ghe.example/api/v3/repos/old/name");
    assert_eq!(routed(&metadata, &new).expect("routed"), new);
}

#[test]
fn a_routed_path_keeps_its_encoding() {
    let new = url("https://api.github.com/repositories/42");
    let nested = url(
        "https://api.github.com/repos/old/name/contents/packages%2Freact%2Fa%20b.json?ref=main",
    );
    assert_eq!(
        routed(&nested, &new).expect("routed").as_str(),
        "https://api.github.com/repositories/42/contents/packages%2Freact%2Fa%20b.json?ref=main"
    );
}

#[test]
fn a_read_names_its_commit_and_whether_it_is_a_path_inside_it() {
    let sha = "A".repeat(40);
    let at = |text: String| commit_read(&url(&text));
    let path = at(format!(
        "https://api.github.com/repos/O/R/contents/docs/a.md?ref={sha}"
    ))
    .expect("contents");
    assert_eq!(path.repository, "https://api.github.com/repos/o/r");
    assert_eq!(path.sha, sha.to_ascii_lowercase());
    assert!(path.path && path.contents);
    let root = at(format!(
        "https://api.github.com/repos/o/r/contents?ref={sha}"
    ));
    assert!(!root.expect("root").path);
    let tree_path = at(format!(
        "https://api.github.com/repositories/9/git/trees/{sha}:docs"
    ))
    .expect("tree path");
    assert_eq!(
        tree_path.repository,
        "https://api.github.com/repositories/9"
    );
    assert!(tree_path.path && !tree_path.contents);
    let encoded = at(format!(
        "https://api.github.com/repos/o/r/git/trees/{sha}%3Adocs"
    ));
    assert!(encoded.expect("encoded").path);
    let tree = at(format!("https://api.github.com/repos/o/r/git/trees/{sha}"));
    assert!(!tree.expect("tree").path);
    let commit = at(format!("https://api.github.com/repos/o/r/commits/{sha}"));
    assert!(!commit.expect("commit").path);
    for none in [
        format!("https://api.github.com/repos/o/r/git/blobs/{sha}"),
        format!("https://api.github.com/repos/o/r/compare/{sha}...{sha}"),
        "https://api.github.com/repos/o/r/contents/a.md?ref=main".to_owned(),
        format!("https://api.github.com/repos/o/r/commits/{sha}/pulls"),
    ] {
        assert_eq!(at(none.clone()), None, "{none}");
    }
}

#[test]
fn a_redirect_to_another_resource_is_no_route() {
    let from = url("https://api.github.com/repos/a/b/contents/x");
    let moved = url("https://api.github.com/repositories/42/contents/y");
    assert!(rename_route(&from, &moved).is_none());
    let elsewhere = url("https://api.github.com/repos/c/d/contents/x");
    assert!(rename_route(&from, &elsewhere).is_none());
}
