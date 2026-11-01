//! MCP progress: a call whose request carries `_meta.progressToken` runs in
//! a notice scope with a progress listener
//! ([`octocode_io::notice::Scope::with_progress`]), and every line its work
//! reports (a `graphBuild`'s stages and parsed-file counts) is sent to the
//! client as `notifications/progress` under that token, in order, before
//! the call's result.
use rmcp::model::{ProgressNotificationParam, ProgressToken};
use std::future::Future;

/// Where one call's progress notifications go.
pub(super) trait ProgressPeer {
    fn notify(&self, param: ProgressNotificationParam) -> impl Future<Output = ()> + Send;
}

impl ProgressPeer for rmcp::Peer<rmcp::RoleServer> {
    async fn notify(&self, param: ProgressNotificationParam) {
        // A client that went away misses the line; the call goes on.
        let _ = self.notify_progress(param).await;
    }
}

/// Run `call`, relaying the progress it reports to `peer` under `token`
/// (each line one notification, numbered from 1, the line as its message).
/// Without a token the call runs as it is and reports to no one.
pub(super) async fn relayed<T>(
    token: Option<ProgressToken>,
    peer: &impl ProgressPeer,
    call: impl Future<Output = T>,
) -> T {
    let Some(token) = token else {
        return call.await;
    };
    let (lines, mut received) = tokio::sync::mpsc::unbounded_channel::<String>();
    let scope = octocode_io::notice::Scope::with_progress(move |line| {
        let _ = lines.send(line.to_owned());
    });
    let mut sent = 0_u32;
    let mut send = async |line: String| {
        sent += 1;
        let param =
            ProgressNotificationParam::new(token.clone(), f64::from(sent)).with_message(line);
        peer.notify(param).await;
    };
    let call = scope.run(call);
    tokio::pin!(call);
    loop {
        tokio::select! {
            biased;
            Some(line) = received.recv() => send(line).await,
            output = &mut call => {
                // Every line the call reported reaches the client before its
                // result does.
                while let Ok(line) = received.try_recv() {
                    send(line).await;
                }
                return output;
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
}
