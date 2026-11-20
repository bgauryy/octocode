//! A recorded sparse checkout seen by the local tools: a path its scope
//! leaves out is a miss that names the scope and leads to the clone that
//! checks it out; a search or listing inside it says what is on disk.
use crate::test_support::{ls_query, policy_for, search_in};
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_tools_shared::gh_git::clone_scope::{CloneRecord, write_record};
use serde_json::json;

/// A workspace holding `clone/`, a checkout of `src` only, as ghCloneRepo
/// records one.
fn workspace() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("workspace");
    let clone = root.path().join("clone");
    std::fs::create_dir_all(clone.join(".git")).expect("git dir");
    std::fs::create_dir_all(clone.join("src")).expect("src");
    std::fs::write(clone.join("src/lib.rs"), "fn squeeze() {}\n").expect("file");
    let git_dir = octocode_engine::search::walk::checkout_git_dir(&clone).expect("checkout");
    write_record(
        &git_dir,
        &CloneRecord {
            owner: "sharkdp".into(),
            repo: "hexyl".into(),
            commit_sha: "6ecc29b9c8c84d08a7e860f7f69c22b113b480ea".into(),
            paths: Some(vec!["src".into()]),
            root_files: true,
            history_depth: 1,
        },
    )
    .expect("record");
    root
}

/// The ghCloneRepo calls among an error's or a row's leads.
fn reclones(calls: &serde_json::Value) -> Vec<serde_json::Value> {
    calls
        .as_object()
        .into_iter()
        .flat_map(|calls| calls.values())
        .filter(|call| call["tool"] == "ghCloneRepo")
        .cloned()
        .collect()
}

fn assert_recloned(calls: &serde_json::Value) {
    let [call] = reclones(calls)
        .try_into()
        .unwrap_or_else(|calls: Vec<_>| panic!("one ghCloneRepo lead: {calls:?}"));
    let query = &call["input"]["queries"][0];
    assert_eq!(query["paths"], json!(["src", "tests"]), "{call}");
    assert_eq!(
        query["ref"], "6ecc29b9c8c84d08a7e860f7f69c22b113b480ea",
        "{call}"
    );
}

#[test]
fn a_search_outside_the_sparse_paths_names_them_and_leads_to_the_clone() {
    let root = workspace();
    let (policy, security) = policy_for(root.path());
    let error = search_in(
        &ls_query(
            json!({"path": "clone/tests", "matchString": ["squeeze"]}),
            None,
        ),
        &policy,
        &security,
        None,
    )
    .expect_err("outside the checkout");
    assert!(
        error
            .message
            .contains("outside this checkout's sparse paths (src)"),
        "{}",
        error.message
    );
    assert_recloned(error.calls.as_deref().expect("leads"));
    // A missing path inside the scope is missing at the commit: plain.
    let error = search_in(
        &ls_query(
            json!({"path": "clone/src/gone", "matchString": ["squeeze"]}),
            None,
        ),
        &policy,
        &security,
        None,
    )
    .expect_err("missing inside the scope");
    assert!(!error.message.contains("sparse"), "{}", error.message);
}

#[test]
fn a_search_inside_a_sparse_checkout_says_what_is_on_disk() {
    let root = workspace();
    let (policy, security) = policy_for(root.path());
    for pattern in ["squeeze", "nowhere-here"] {
        let found = search_in(
            &ls_query(json!({"path": "clone", "matchString": [pattern]}), None),
            &policy,
            &security,
            None,
        )
        .expect("search");
        assert!(
            found
                .warnings
                .iter()
                .any(|warning| warning.contains("This checkout is sparse")),
            "{pattern}: {:?}",
            found.warnings
        );
    }
}

#[test]
fn a_listing_outside_or_inside_the_sparse_paths_says_so() {
    let root = workspace();
    let (policy, security) = policy_for(root.path());
    let list = |path: &str| {
        let query = serde_json::from_value(json!({"path": path, "operation": "files"}))
            .expect("structureSearch row");
        crate::structure_search::execute_structure(&query, &policy, &security, &NeverCancel, None)
    };
    let error = list("clone/tests").expect_err("outside the checkout");
    assert!(
        error.message.contains("sparse paths (src)"),
        "{}",
        error.message
    );
    assert_recloned(error.calls.as_deref().expect("leads"));
    let listed = list("clone").expect("listing");
    assert!(
        listed["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|warning| warning
                .as_str()
                .is_some_and(|text| text.contains("This checkout is sparse"))),
        "{listed}"
    );
}

#[test]
fn a_read_outside_the_sparse_paths_leads_to_the_clone() {
    let root = workspace();
    let (policy, security) = policy_for(root.path());
    let query: octocode_contract_types::LocalFetchQuery =
        serde_json::from_value(json!({"path": "clone/tests/integration_tests.rs"}))
            .expect("localFetch row");
    let read = crate::local_fetch::execute_local_fetch(
        &query,
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
    );
    let error = read.error.as_deref().unwrap_or_default();
    assert!(error.contains("sparse paths (src)"), "{error}");
    let next = serde_json::to_value(read.next.as_ref().expect("leads")).expect("json");
    assert_recloned(&next);
}
