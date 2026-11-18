//! Fixtures shared by the tools' tests, here because they need this crate's
//! selection and source views; the generic ones are
//! `octocode_runtime_base::test_support`.
#![expect(
    clippy::panic,
    reason = "fixtures for tests, compiled only for them (`test`, `test-support`): a broken fixture fails its test at once"
)]
pub mod selection_fake;
pub mod selection_runtime;

/// The view a localFetch or ghGetFileContent lead `row` reads, decoded
/// through the tool's generated query type: an omitted `view` is its
/// contract default, as the call that runs the row reads it.
#[track_caller]
pub fn read_view(
    tool: octocode_runtime_base::id::ToolId,
    row: &serde_json::Value,
) -> octocode_contract_types::ReadView {
    use octocode_runtime_base::id::ToolId;
    use octocode_runtime_base::response::result::wire_query;
    let local = match tool {
        ToolId::LocalFetch => wire_query(row.clone()),
        ToolId::GhGetFileContent => wire_query(row.clone())
            .and_then(|query| crate::source_view::local_query::local_query(&query).ok()),
        other => panic!("{} rows read no source view", other.as_str()),
    };
    let local = local.unwrap_or_else(|| panic!("{} row {row} is not a read", tool.as_str()));
    crate::source_view::local_fetch_query::view(&local)
}
