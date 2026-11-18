use super::*;
use rmcp::model::NumberOrString;
use std::sync::{Arc, Mutex};

/// A fake client that records what it was sent.
#[derive(Clone, Default)]
struct Client(Arc<Mutex<Vec<ProgressNotificationParam>>>);

impl ProgressPeer for Client {
    async fn notify(&self, param: ProgressNotificationParam) {
        self.0.lock().expect("sent").push(param);
    }
}

/// Lines a call reports, on its own task or on work carried to another
/// thread, reach the client as numbered progress under the request's
/// token, all of them before the call returns; a call without a token
/// sends nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_calls_progress_reaches_the_client_under_its_token() {
    let client = Client::default();
    let token = ProgressToken(NumberOrString::String("build-1".into()));
    let report = |line: &str| {
        let listener = octocode_io::notice::Scope::current()
            .and_then(|scope| scope.progress())
            .expect("the call's listener");
        listener(line);
    };
    let output = relayed(Some(token.clone()), &client, async {
        report("graphBuild: scanning");
        let carried = octocode_io::notice::carried(move || {
            report("graphBuild: parsed 2/2 files; linking");
        });
        tokio::task::spawn_blocking(carried).await.expect("worker");
        7
    })
    .await;
    assert_eq!(output, 7);
    let sent = client.0.lock().expect("sent").clone();
    let shown = sent
        .iter()
        .map(|param| (param.progress, param.message.clone().unwrap_or_default()))
        .collect::<Vec<_>>();
    assert_eq!(
        shown,
        [
            (1.0, "graphBuild: scanning".to_owned()),
            (2.0, "graphBuild: parsed 2/2 files; linking".to_owned()),
        ]
    );
    assert!(sent.iter().all(|param| param.progress_token == token));

    let silent = Client::default();
    let output = relayed(None, &silent, async {
        assert!(
            octocode_io::notice::Scope::current()
                .and_then(|scope| scope.progress())
                .is_none()
        );
        1
    })
    .await;
    assert_eq!(output, 1);
    assert!(silent.0.lock().expect("sent").is_empty());
}
