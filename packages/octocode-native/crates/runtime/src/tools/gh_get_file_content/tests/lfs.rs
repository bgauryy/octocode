//! Git LFS pointers: the row warns and serves the pointer text as it is.
use super::*;

const SPEC: &str = "version https://git-lfs.github.com/spec/v1";

async fn served(body: &str) -> GhGetFileContentResult {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/pp.bin",
        200,
        json!({"type":"file","encoding":"base64","content":STANDARD.encode(body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    read(&provider, json!({"owner":"a","repo":"b","path":"pp.bin","ref":"0123456789abcdef0123456789abcdef01234567"})).await
}

#[tokio::test]
async fn a_pointer_blob_warns_with_its_object_size_and_stays_as_served() {
    let size: u64 = 889_000_000;
    let body = format!("{SPEC}\noid sha256:{}\nsize {size}\n", "ab".repeat(32));
    let row = served(&body).await;
    let warning = row
        .file
        .content
        .warnings
        .iter()
        .find(|w| w.contains("LFS pointer"))
        .unwrap_or_else(|| panic!("no warning: {:?}", row.file.content.warnings));
    assert!(warning.contains(&size.to_string()), "{warning}");
    assert!(warning.contains("media.githubusercontent.com"), "{warning}");
    assert_eq!(row.file.content.content.as_deref(), Some(body.as_str()));
}

#[test]
fn the_media_link_encodes_each_path_segment() {
    let pointer = crate::tools::gh_get_file_content::lfs::Pointer { size: 5 };
    let warning =
        crate::tools::gh_get_file_content::lfs::warning(&pointer, "a", "b", "main", "dir/pp file#1?.bin");
    assert!(
        warning.ends_with("https://media.githubusercontent.com/media/a/b/main/dir/pp%20file%231%3F.bin"),
        "{warning}"
    );
}

#[tokio::test]
async fn a_file_that_only_mentions_the_spec_is_not_a_pointer() {
    for body in [
        format!(
            "# Notes\nPointers start with `{SPEC}`.\noid sha256:{}\nsize 5\n",
            "ab".repeat(32)
        ),
        format!("{SPEC}\nsize 5\n"),
        format!("{SPEC}\noid sha256:xyz\nsize 5\n"),
    ] {
        let row = served(&body).await;
        assert!(
            !row.file.content.warnings.iter().any(|w| w.contains("LFS")),
            "{:?}",
            row.file.content.warnings
        );
    }
}
