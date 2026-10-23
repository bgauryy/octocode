//! GitHub `/tree` and `/blob` URL readers.
use super::{github_repo_dir, release_ref_in};

/// Both readers of a `/tree/<ref>` or `/blob/<ref>` URL drop its query
/// and fragment and read nothing from another page kind.
#[test]
fn tree_and_blob_urls_name_a_directory_or_a_release_ref() {
    let dir = |url| github_repo_dir(url);
    assert_eq!(
        dir("https://github.com/o/r/tree/main/a/b?x=1").as_deref(),
        Some("a/b")
    );
    assert_eq!(
        dir("https://github.com/o/r/blob/main/a/README.md#top").as_deref(),
        Some("a")
    );
    assert_eq!(dir("https://github.com/o/r/blob/main/README.md"), None);
    assert_eq!(dir("https://github.com/o/r/issues/1/x"), None);
    assert_eq!(dir("https://github.com/o/r"), None);
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
