//! Opening the source: a FIFO is refused without blocking.

use super::*;

/// A FIFO is opened without blocking and refused by the handle's own
/// stat, so a path swapped for one after the policy check cannot hang
/// the read; a regular file resolves with the opened handle.
#[test]
fn a_fifo_is_refused_without_blocking_and_a_file_opens() {
    let root = tempfile::tempdir().expect("root");
    let fifo = root.path().join("pipe.txt");
    crate::tools::test_support::make_fifo(&fifo);
    std::fs::write(root.path().join("a.txt"), "x\n").expect("file");
    let (paths, _) = crate::tools::test_support::simple_policy(root.path());
    let query = |path: &std::path::Path| LocalFetchQuery {
        path: path.to_string_lossy().parse().expect("path"),
        ..test_local_fetch_query()
    };
    let refused = resolve_file(&query(&fifo), &paths).expect_err("a FIFO is no file");
    assert_eq!(refused.error_code, Some(ErrorCode::NotAFile));
    let (_, _, meta) =
        resolve_file(&query(&root.path().join("a.txt")), &paths).expect("regular file");
    assert_eq!(meta.len(), 2);
}

/// A FIFO is `notAFile`, named by the requested path, with nothing to
/// retry: no lead and no "retry the exact path" tip.
#[test]
fn a_fifo_is_not_a_file_named_by_its_path_with_nothing_to_retry() {
    let root = tempfile::tempdir().expect("root");
    let fifo = root.path().join("pipe1");
    crate::tools::test_support::make_fifo(&fifo);
    let paths = crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
        workspace_root: Some(root.path().to_path_buf()),
        ..Default::default()
    })
    .expect("policy");
    let requested = fifo.to_string_lossy().into_owned();
    let result = execute_local_fetch(
        &LocalFetchQuery {
            path: requested.parse().expect("path"),
            ..test_local_fetch_query()
        },
        &paths,
        &crate::security::ContentSecurity::new(),
        &crate::execution::cancel::NeverCancel,
        None,
        None,
    );
    assert_eq!(result.error_code, Some(ErrorCode::NotAFile), "{result:?}");
    let message = result.error.as_deref().unwrap_or_default();
    assert!(message.starts_with(&requested), "{message}");
    assert!(
        result.next.is_none() && result.hints.is_empty(),
        "{result:?}"
    );
    use crate::response::output::ToolOutput;
    assert_eq!(
        crate::tools::local_fetch::output::Output.error_hint(ErrorCode::NotAFile),
        Some(crate::response::output::NO_HINT)
    );
}
