//! A file named after a credential is listed with a placeholder in its name,
//! and that shown name reads the file: through the input gate, structureSearch
//! and localFetch alike.

use crate::execution::cancel::NeverCancel;
use crate::id::ToolId;
use crate::security::{ContentSecurity, InputPolicy};
use crate::tools::test_support::{Fixture, workspace_policy};
use octocode_contract_types::{LocalFetchQuery, StructureSearchQuery};
use serde_json::{Value, json};

const TOKEN: &str = "ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8";
const SHOWN: &str = "deploy-[REDACTED-GITHUBTOKENS].txt";

fn fixture() -> Fixture {
    let dir = Fixture::new();
    dir.write(
        &format!("conf/deploy-{TOKEN}.txt"),
        "hello from the deploy file\n",
    );
    dir.write("conf/plain.txt", "plain\n");
    dir
}

fn structure(dir: &Fixture, row: Value) -> Value {
    let row = crate::tools::test_support::opened(ToolId::StructureSearch, &row);
    let query: StructureSearchQuery = serde_json::from_value(row).expect("typed row");
    crate::tools::structure_search::execute_structure(
        &query,
        &workspace_policy(&dir.0),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    )
    .unwrap_or_else(|error| panic!("{error:?}"))
}

fn fetch(dir: &Fixture, path: &str) -> Value {
    let row = crate::tools::test_support::opened(ToolId::LocalFetch, &json!({"path": path}));
    let query: LocalFetchQuery = serde_json::from_value(row).expect("typed row");
    let result = crate::tools::local_fetch::execute_local_fetch(
        &query,
        &workspace_policy(&dir.0),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        None,
    );
    serde_json::to_value(result).expect("json")
}

#[test]
fn the_input_gate_admits_a_local_path_named_after_a_credential() {
    let security = ContentSecurity::new();
    let row = json!({"path": format!("conf/deploy-{TOKEN}.txt")});
    assert!(
        security
            .check_input(row.clone(), InputPolicy::LocalQuery)
            .is_ok()
    );
    assert!(security.check_input(row, InputPolicy::Query).is_err());
    assert!(
        security
            .check_input(
                json!({"path": format!("conf/{SHOWN}")}),
                InputPolicy::LocalQuery
            )
            .is_ok()
    );
}

#[test]
fn a_listing_shows_the_placeholder_and_the_shown_name_reads_the_file() {
    let dir = fixture();
    let listing = structure(&dir, json!({"operation": "files", "path": "conf"}));
    let text = listing.to_string();
    assert!(text.contains(SHOWN), "{text}");
    assert!(!text.contains(TOKEN), "{text}");
    for path in [format!("conf/{SHOWN}"), format!("conf/deploy-{TOKEN}.txt")] {
        let read = fetch(&dir, &path);
        assert!(
            read.to_string().contains("hello from the deploy file"),
            "{path}: {read}"
        );
    }
}

#[test]
fn a_shown_name_that_matches_no_entry_is_an_ordinary_missing_file() {
    let dir = fixture();
    let read = fetch(&dir, "conf/nothing-[REDACTED-GITHUBTOKENS].txt");
    assert!(read.to_string().contains("notFound"), "{read}");
}
