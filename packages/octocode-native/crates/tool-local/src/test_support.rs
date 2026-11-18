//! Fixtures this crate's tests share with the tests that run its tools
//! from outside it.
#![expect(
    clippy::expect_used,
    reason = "fixtures for tests, compiled only for them (`test`, `test-support`): a broken fixture fails its test at once"
)]
use crate::local_search::{LocalSearchResult, execute_local_search};
use octocode_contract_types::LocalSearchQuery;
use octocode_runtime_base::{
    execution::cancel::NeverCancel, policy::path::PathPolicy, security::ContentSecurity,
};

/// Test query from wire JSON over a base query (or a minimal valid one);
/// `null` removes a field so its contract default applies.
pub fn ls_query(fields: serde_json::Value, base: Option<&LocalSearchQuery>) -> LocalSearchQuery {
    let mut value = base.map_or_else(
        || serde_json::json!({"path": "_", "matchString": ["_"]}),
        |base| serde_json::to_value(base).expect("query serializes"),
    );
    let object = value.as_object_mut().expect("query object");
    for (key, field) in fields.as_object().expect("fields object") {
        if field.is_null() {
            object.remove(key);
        } else {
            object.insert(key.clone(), field.clone());
        }
    }
    serde_json::from_value(value).expect("valid localSearch query")
}

/// One never-cancelled localSearch call under the response `window`.
pub fn search_in(
    request: &LocalSearchQuery,
    policy: &PathPolicy,
    security: &ContentSecurity,
    window: Option<usize>,
) -> Result<LocalSearchResult, octocode_runtime_base::response::result::ToolError> {
    execute_local_search(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        request,
        policy,
        security,
        &NeverCancel,
        window,
        None,
    )
}

pub fn policy_for(root: &std::path::Path) -> (PathPolicy, ContentSecurity) {
    let policy = octocode_runtime_base::test_support::workspace_policy(root);
    (policy, ContentSecurity::new())
}
