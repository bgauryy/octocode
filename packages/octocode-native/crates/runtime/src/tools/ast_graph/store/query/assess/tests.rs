use super::*;

struct Cancelled;
impl crate::execution::cancel::CancellationCheck for Cancelled {
    fn check(&self) -> Result<(), String> {
        Err("cancelled".into())
    }
}

/// The request's cancellation reaches the git run: it is reported as
/// cancelled, not as a git failure, and no fixed timeout decides it.
#[test]
fn cancellation_stops_the_git_run_as_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let error = git_changed(dir.path(), "HEAD", &Cancelled).unwrap_err();
    assert_eq!(error.value["errorCode"], json!(ErrorCode::Cancelled));
}
