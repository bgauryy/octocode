//! The source limits at their real sizes, once each through the public call: unit tests size
//! their fixtures from small limits, so these are what prove the limits every build serves.

use crate::support::{Workspace, call, row_data};
use octocode_native::security::SourceLimits;
use serde_json::json;

/// A source of `bytes` short lines holding `needle` on its first line.
fn source(bytes: usize) -> Vec<u8> {
    let mut text = b"fn needle() {}\n".to_vec();
    text.resize(bytes - 1, b'x');
    text.push(b'\n');
    text
}

/// A source of exactly the whole-file ceiling is read whole, so a `block` read parses it; one
/// byte more streams, and the same read is refused for needing the whole file.
#[tokio::test]
async fn the_whole_file_ceiling_is_the_real_one() {
    let whole = SourceLimits::REAL.whole_source;
    let workspace = Workspace::new();
    let at = workspace.write("at.rs", source(whole));
    let past = workspace.write("past.rs", source(whole + 1));
    let runtime = workspace.runtime(&[]);
    let read = |path| {
        call(
            &runtime,
            "localFetch",
            json!({"path":path,"matchString":["needle"],"block":true}),
        )
    };
    let at = read(at).await.expect("read at the ceiling");
    assert!(row_data(&at)["errorCode"].is_null(), "{at:?}");
    let past = read(past).await.expect("read past the ceiling");
    assert_eq!(
        row_data(&past)["errorCode"],
        "largeSourceWindowOnly",
        "{past:?}"
    );
    runtime.close().await;
}
