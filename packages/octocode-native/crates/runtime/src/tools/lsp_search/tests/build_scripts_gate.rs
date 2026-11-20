//! `lspSearch` starts the Rust server it routes a file to: what that server
//! runs is its own configuration's, never a query's. The server here is a
//! script that records every start: a marker file is the proof a call
//! started one, whatever the call's row says.
use super::fetched_code::{World, call};
use serde_json::json;

#[tokio::test]
async fn a_plain_query_starts_the_routed_server() {
    let world = World::new();
    let runtime = world.runtime(&[], &json!({}), None);
    let plain =
        json!({"path": world.cwd.join("checkout/src/lib.rs"), "operation": "documentSymbols"});
    let row = call(&runtime, plain).await;
    assert_ne!(row["errorCode"], "policy", "{row}");
    assert!(world.started(), "{row}");
}
