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
mod tests;
