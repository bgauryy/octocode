use super::*;
use octocode_runtime_base::policy::{PolicyError, PolicyErrorCode};

/// No path policy for the root means no test-code rule: the build fails
/// with that error instead of labelling roles by spelling.
#[test]
fn a_failed_path_policy_fails_the_projection_instead_of_falling_back_to_spelling() {
    let failure = PolicyError::new(PolicyErrorCode::Io, "root unreadable");
    let error = test_code_index(Err(failure.clone()))
        .err()
        .expect("no test-code rule without a path policy");
    assert_eq!(error.code, failure.shared_code());
    assert!(
        error.message.contains("root unreadable"),
        "{}",
        error.message
    );
}
