use super::*;
use crate::process_fixture::{Holder, Leader, Tree, gone_within, kill};

#[tokio::test]
async fn cancelling_a_token_lookup_leaves_no_descendant() {
    let (command, tree) = Tree::new(Leader::Sleep, Holder::Silent, false);
    let mut command = tokio::process::Command::from(command);
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let budget = RequestBudget::with_timeout(Duration::from_secs(30), 1);
    let cancel = budget.cancellation.clone();
    let (result, pid) = tokio::join!(run_command(command, &budget), async {
        let pid = tokio::task::spawn_blocking(move || tree.holder_pid())
            .await
            .expect("pid");
        cancel.cancel();
        pid
    });
    let gone = gone_within(pid, Duration::from_secs(5));
    kill(pid);
    assert_eq!(
        result.unwrap_err().kind,
        octocode_github::ProviderErrorKind::Cancelled
    );
    assert!(gone, "descendant {pid} survived token cancellation");
}
