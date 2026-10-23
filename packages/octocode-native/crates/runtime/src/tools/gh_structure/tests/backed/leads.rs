//! Materialized pages and the reads a listing leads to.
use super::*;

#[tokio::test]
async fn materialize_skips_binary_files_with_a_warning() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!([
            {"name":"img.png","path":"img.png","type":"file","size":4},
            {"name":"ok.rs","path":"ok.rs","type":"file","size":4}
        ]),
    )
    .await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/commits/{sha}"),
        200,
        json!({"sha":sha}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/img.png",
        200,
        json!({
            "type":"file","encoding":"base64","content":STANDARD.encode([0u8, 1, 2, 3])
        }),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/ok.rs",
        200,
        json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("fn x(){}")
        }),
    )
    .await;
    let home = crate::tools::test_support::Fixture::new();
    let out = run_at(
            &server,
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":sha,"materialize":true}),
            &home.0,
        )
        .await
        .expect("materialize continues past binary files");
    let local = out.data["location"]["localPath"]
        .as_str()
        .expect("location");
    assert!(std::path::Path::new(local).join("ok.rs").exists());
    // The warning carries the count; `location.skipped` names the file.
    assert_eq!(
        out.data["location"]["skipped"],
        json!(["img.png"]),
        "{}",
        out.data
    );
    let warnings = out.data["warnings"].to_string();
    assert!(
        warnings.contains("Skipped 1 unreadable file(s)"),
        "{}",
        out.data
    );
}

/// A listing leads to the read a caller makes next: the package's
/// entry file (outlined) in a wide directory, at the listed commit.
#[tokio::test]
async fn a_listing_leads_to_an_outline_read_of_its_entry_file() {
    let server = MockServer::builder().start().await;
    let files = (0..12)
            .map(|n| json!({"name":format!("m{n}.py"),"path":format!("pkg/m{n}.py"),"type":"file","size":4}))
            .chain(std::iter::once(json!({"name":"__init__.py","path":"pkg/__init__.py","type":"file","size":4})))
            .collect::<Vec<_>>();
    mount_listing(&server, "pkg", json!(files)).await;
    let out = run(
        &server,
        json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"pkg"}),
    )
    .await
    .expect("listing");
    let read = &out.data["next"]["read"];
    assert_eq!(read["tool"], "ghGetFileContent", "{}", out.data);
    let row = &read["query"]["queries"][0];
    assert_eq!(row["path"], "pkg/__init__.py", "{}", out.data);
    assert_eq!(row["ref"], SHA, "{}", out.data);
    assert_eq!(row["minify"], "symbols", "{}", out.data);
}

/// A materialized listing points at the listed directory on disk.
#[tokio::test]
async fn materialize_leads_into_the_local_copy_of_the_listed_directory() {
    let server = MockServer::builder().start().await;
    mount_listing(
        &server,
        "src",
        json!([{"name":"lib.rs","path":"src/lib.rs","type":"file","size":8}]),
    )
    .await;
    mount_content(&server, "src/lib.rs", "fn x(){}").await;
    let home = crate::tools::test_support::Fixture::new();
    let out = run_at(
            &server,
            json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"src","materialize":true}),
            &home.0,
        )
        .await
        .expect("materialize");
    let local = out.data["location"]["localPath"]
        .as_str()
        .expect("location")
        .to_owned();
    assert!(local.ends_with("/src"), "{}", out.data);
    assert!(std::path::Path::new(&local).join("lib.rs").exists());
    // No lead re-lists the tree this response just returned.
    assert!(
        out.data.pointer("/next/exploreClone").is_none(),
        "{}",
        out.data
    );
}
