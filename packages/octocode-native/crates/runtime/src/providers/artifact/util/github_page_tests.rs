//! GitHub `/tree` and `/blob` URL readers.
use super::{github_ref_path, release_ref_in};

/// Both readers of a `/tree/<ref>` or `/blob/<ref>` URL drop its query
/// and fragment and read nothing from another page kind.
#[test]
fn tree_and_blob_urls_name_a_directory_or_a_release_ref() {
    let path = |url| github_ref_path(url).map(|segments| segments.join("/"));
    assert_eq!(
        path("https://github.com/o/r/tree/main/a/b?x=1").as_deref(),
        Some("main/a/b")
    );
    assert_eq!(
        path("https://github.com/o/r/blob/main/a/README.md#top").as_deref(),
        Some("main/a")
    );
    assert_eq!(path("https://github.com/o/r/blob/main/README.md"), None);
    assert_eq!(path("https://github.com/o/r/issues/1/x"), None);
    assert_eq!(path("https://github.com/o/r"), None);
    assert_eq!(
        path("https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore").as_deref(),
        Some("master/crates/ignore")
    );
    assert_eq!(
        path("https://github.com/o/r/tree/main/packages/x/").as_deref(),
        Some("main/packages/x")
    );
    assert_eq!(path("https://github.com/o/r/tree/main"), None);
    assert_eq!(path("https://github.com/o/r#readme"), None);
    assert_eq!(path("git+https://github.com/o/r.git"), None);
    let release = |url| release_ref_in(url, "1.2.0");
    assert_eq!(
        release("https://github.com/o/r/tree/v1.2.0#x").as_deref(),
        Some("v1.2.0")
    );
    assert_eq!(
        release("https://github.com/o/r/blob/pkg@1.2.0").as_deref(),
        Some("pkg@1.2.0")
    );
    assert_eq!(release("https://github.com/o/r/tree/v1.2.0/docs"), None);
    assert_eq!(release("https://github.com/o/r/tree/main"), None);
    assert_eq!(release("https://github.com/o/r/releases/v1.2.0"), None);
}
