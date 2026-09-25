//! Internal delivery port. Adapters own vendor I/O; the dispatcher alone owns DB state.
use super::{Codex, claude, grok::Grok, opencode::OpenCode};
use anyhow::{Result, bail};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTransport {
    Claude,
    Codex,
    Grok,
    OpenCode,
}
impl NativeTransport {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "grok" => Ok(Self::Grok),
            "opencode" => Ok(Self::OpenCode),
            _ => bail!("Unknown native transport: {value}; raw delivery is owned by its host"),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Grok => "grok",
            Self::OpenCode => "opencode",
        }
    }
    pub fn requires_action(self) -> bool {
        matches!(self, Self::Claude | Self::Grok)
    }
    pub fn receipt(self) -> ReceiptKind {
        match self {
            Self::Claude => ReceiptKind::SocketWrite,
            Self::Codex => ReceiptKind::JsonRpc,
            Self::Grok => ReceiptKind::TurnCompletion,
            Self::OpenCode => ReceiptKind::Http,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptKind {
    SocketWrite,
    JsonRpc,
    TurnCompletion,
    Http,
}
impl ReceiptKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::SocketWrite => "socket-write-only",
            Self::JsonRpc => "jsonrpc",
            Self::TurnCompletion => "acp-turn-completion",
            Self::Http => "http",
        }
    }
}

/// The already-staged DB batch, rendered once. No recipient conversation history.
pub struct Offer<'a> {
    pub content: &'a str,
    pub token: &'a str,
    pub action: bool,
}
pub struct Receipt {
    pub kind: ReceiptKind,
    pub turn_requested: bool,
    pub usage: Option<Value>,
    pub stop_reason: Option<Value>,
}
pub enum Progress {
    Pending { turn_requested: bool },
    Submitted(Receipt),
}
pub enum Readiness {
    Ready,
    Deferred,
}

enum Backend {
    Claude,
    Codex(Box<Codex>),
    Grok(Grok),
    OpenCode(OpenCode),
}
pub struct NativeDelivery {
    mode: NativeTransport,
    endpoint: String,
    session: String,
    workspace: String,
    backend: Backend,
}
impl NativeDelivery {
    pub fn matches(
        &self,
        mode: NativeTransport,
        endpoint: &str,
        session: &str,
        workspace: &str,
    ) -> bool {
        self.mode == mode
            && self.endpoint == endpoint
            && self.session == session
            && self.workspace == workspace
    }
    pub fn connect(
        mode: NativeTransport,
        endpoint: &str,
        session: &str,
        workspace: &str,
    ) -> Result<Self> {
        let backend = match mode {
            // Claude has no read-only metadata API. The write validates its same-user socket.
            NativeTransport::Claude => Backend::Claude,
            NativeTransport::Codex => Backend::Codex(Box::new(Codex::connect(endpoint)?)),
            NativeTransport::Grok => Backend::Grok(Grok::connect(endpoint, session, workspace)?),
            NativeTransport::OpenCode => {
                Backend::OpenCode(OpenCode::connect(endpoint, session, workspace)?)
            }
        };
        Ok(Self {
            mode,
            endpoint: endpoint.into(),
            session: session.into(),
            workspace: workspace.into(),
            backend,
        })
    }
    pub fn prepare(&mut self) -> Result<Readiness> {
        let ready = match &mut self.backend {
            Backend::Codex(client) => client.idle(&self.session, &self.workspace)?,
            Backend::OpenCode(client) => client.idle()?,
            Backend::Claude | Backend::Grok(_) => true,
        };
        Ok(if ready {
            Readiness::Ready
        } else {
            Readiness::Deferred
        })
    }
    /// One effect. Errors after this boundary are uncertain and must not cause replay.
    pub fn offer(&mut self, offer: Offer<'_>) -> Result<Progress> {
        if self.mode.requires_action() && !offer.action {
            bail!("{} cannot inject passive-only context", self.mode.name());
        }
        match &mut self.backend {
            Backend::Claude => claude(&self.endpoint, &self.session, offer.token, offer.content)?,
            Backend::Codex(client) => {
                if offer.action {
                    client.start_turn(&self.session, offer.content)?;
                } else {
                    client.inject(&self.session, offer.content)?;
                }
            }
            Backend::OpenCode(client) => client.submit(offer.content, offer.action)?,
            Backend::Grok(client) => {
                client.submit(offer.content, offer.token)?;
                return Ok(Progress::Pending {
                    turn_requested: true,
                });
            }
        }
        Ok(Progress::Submitted(Receipt {
            kind: self.mode.receipt(),
            turn_requested: offer.action,
            usage: None,
            stop_reason: None,
        }))
    }
    /// Observe the same offer; never resend it while waiting for a vendor receipt.
    pub fn poll(&mut self, token: &str) -> Result<Progress> {
        let Backend::Grok(client) = &mut self.backend else {
            bail!("Native adapter has no pending receipt");
        };
        let Some(receipt) = client.poll()? else {
            return Ok(Progress::Pending {
                turn_requested: true,
            });
        };
        Ok(Progress::Submitted(Receipt {
            kind: self.mode.receipt(),
            turn_requested: true,
            usage: grok_usage(&receipt, token),
            stop_reason: Some(receipt["stopReason"].clone()),
        }))
    }
}

fn grok_usage(receipt: &Value, token: &str) -> Option<Value> {
    // Nested usage aggregates the turn; top-level counters cover only its last request.
    let usage = receipt.pointer("/_meta/usage")?.as_object()?;
    let mut record = json!({"key":format!("grok-{token}"),"scope":"turn"});
    let mut observed = false;
    for (field, source) in [
        ("inputTokens", "inputTokens"),
        ("outputTokens", "outputTokens"),
        ("cachedInputTokens", "cachedReadTokens"),
        ("cacheWriteTokens", "cacheCreationTokens"),
    ] {
        if let Some(value) = usage.get(source).and_then(Value::as_u64) {
            record[field] = json!(value);
            observed = true;
        }
    }
    if !observed {
        return None;
    }
    if let Some(model) = receipt.pointer("/_meta/modelId").and_then(Value::as_str)
        && !model.is_empty()
        && model.chars().count() <= 256
    {
        record["model"] = json!(model);
    }
    Some(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passive_only_offer_cannot_reach_an_action_only_adapter() -> Result<()> {
        // No socket exists: the protocol must reject before any native I/O.
        let mut client = NativeDelivery::connect(
            NativeTransport::Claude,
            "/missing.sock",
            "native-id",
            "/repo",
        )?;
        let error = client.offer(Offer {
            content: "FYI",
            token: "token",
            action: false,
        });
        assert!(
            error
                .err()
                .is_some_and(|e| e.to_string().contains("passive-only"))
        );
        assert!(client.poll("token").is_err());
        assert!(NativeTransport::parse("raw").is_err());
        Ok(())
    }

    #[test]
    fn usage_maps_completed_turn_only_and_keeps_missing_counters_unknown() {
        assert!(grok_usage(&json!({"inputTokens":9999}), "t").is_none());
        assert_eq!(
            grok_usage(
                &json!({"inputTokens":9999,"_meta":{"usage":{"inputTokens":42},"modelId":"grok"}}),
                "t"
            ),
            Some(json!({"key":"grok-t","scope":"turn","inputTokens":42,"model":"grok"}))
        );
        assert!(grok_usage(&json!({"_meta":{"usage":{"inputTokens":-1}}}), "t").is_none());
    }
}
