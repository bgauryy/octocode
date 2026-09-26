//! Existing Grok leader transport, pinned to the vendor's version-1 IPC envelope.
//! No leader is spawned, no session is created, and no permission is granted here.
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::Path,
    time::{Duration, Instant},
};

use crate::wire::MAX_FRAME;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TURN_TIMEOUT: Duration = Duration::from_secs(300);

pub fn validate(endpoint: &str) -> Result<()> {
    let path = endpoint.strip_prefix("unix://").unwrap_or(endpoint);
    if !Path::new(path).is_absolute() || path.len() > 103 {
        bail!("Grok endpoint must be an absolute Unix socket path of at most 103 bytes");
    }
    #[cfg(unix)]
    {
        super::owned_socket(path)
    }
    #[cfg(not(unix))]
    {
        bail!("Grok native leader delivery requires Unix; use host hooks on this platform")
    }
}

pub struct Grok {
    #[cfg(unix)]
    socket: std::os::unix::net::UnixStream,
    session: String,
    bytes: Vec<u8>,
    sequence: u64,
    pending: Option<(u64, Instant)>,
}
impl Grok {
    pub fn connect(endpoint: &str, session: &str, workspace: &str) -> Result<Self> {
        validate(endpoint)?;
        uuid::Uuid::parse_str(session)
            .map_err(|_| anyhow!("Grok vendor session must be a UUID"))?;
        #[cfg(unix)]
        {
            let path = endpoint.strip_prefix("unix://").unwrap_or(endpoint);
            let socket = std::os::unix::net::UnixStream::connect(path)?;
            socket.set_read_timeout(Some(Duration::from_millis(1)))?;
            socket.set_write_timeout(Some(Duration::from_secs(5)))?;
            let deadline = Instant::now() + CONNECT_TIMEOUT;
            let mut client = Self {
                socket,
                session: session.to_owned(),
                bytes: Vec::new(),
                sequence: 0,
                pending: None,
            };
            client.send_frame(&json!({"type":"register","client_type":"octocode-communication","mode":"stdio","capabilities":{}}))?;
            let mut registered = false;
            loop {
                if Instant::now() >= deadline {
                    bail!("Grok leader registration timed out");
                }
                let Some(frame) = client.frame()? else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                match frame["type"].as_str() {
                    Some("registered") => {
                        if frame["leader_protocol_version"] != 1 {
                            bail!("Unsupported Grok leader protocol; version 1 is required");
                        }
                        registered = true;
                        if frame["ready"] != false {
                            break;
                        }
                    }
                    Some("leader_ready") if registered => break,
                    Some("error" | "shutdown" | "shutting_down") => {
                        bail!("Grok leader unavailable during registration")
                    }
                    _ => bail!("Unexpected Grok leader registration response"),
                }
            }
            let initialized = client.request("initialize", json!({"protocolVersion":1,"clientInfo":{"name":"octocode-communication","version":env!("CARGO_PKG_VERSION")},"clientCapabilities":{}}), deadline)?;
            if initialized["protocolVersion"] != 1 {
                bail!("Unsupported Grok ACP protocol; version 1 is required");
            }
            let metadata =
                client.request("_x.ai/session/info", json!({"sessionId":session}), deadline)?;
            let info = &metadata["result"];
            let cwd = info["cwd"].as_str().ok_or_else(|| {
                anyhow!("Grok session metadata lacks workspace; no message staged")
            })?;
            if info["sessionId"] != session || std::fs::canonicalize(cwd)? != Path::new(workspace) {
                bail!("Grok session metadata does not match identity/workspace; no message staged");
            }
            // Never resume/load here: Grok replaces the resident session's MCP servers on attach.
            // session/prompt addresses an existing resident session without altering its settings.
            Ok(client)
        }
        #[cfg(not(unix))]
        {
            let _ = workspace;
            bail!("Grok native leader delivery requires Unix")
        }
    }
    fn send_frame(&mut self, frame: &Value) -> Result<()> {
        let bytes = serde_json::to_vec(frame)?;
        if bytes.len() > MAX_FRAME {
            bail!("Grok frame exceeds 8 MiB");
        }
        #[cfg(unix)]
        {
            self.socket.write_all(&(bytes.len() as u32).to_be_bytes())?;
            self.socket.write_all(&bytes)?;
            self.socket.flush()?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            bail!("Grok native leader delivery requires Unix")
        }
    }
    fn send_rpc(&mut self, frame: Value) -> Result<()> {
        self.send_frame(&json!({"type":"acp","payload":frame.to_string()}))
    }
    fn buffered_frame(&mut self) -> Result<Option<Value>> {
        if self.bytes.len() < 4 {
            return Ok(None);
        }
        let size = u32::from_be_bytes(self.bytes[..4].try_into()?) as usize;
        if size > MAX_FRAME {
            bail!("Grok frame exceeds 8 MiB");
        }
        if self.bytes.len() < size + 4 {
            return Ok(None);
        }
        let frame = serde_json::from_slice(&self.bytes[4..size + 4])?;
        self.bytes.drain(..size + 4);
        Ok(Some(frame))
    }
    // Preserve partial headers and bodies across timeouts; never read_exact into a discarded buffer.
    // Drain what the socket already holds until one frame completes: a large frame
    // must not trickle in one small chunk per poll.
    fn frame(&mut self) -> Result<Option<Value>> {
        loop {
            if let Some(frame) = self.buffered_frame()? {
                return Ok(Some(frame));
            }
            #[cfg(unix)]
            {
                let mut chunk = [0u8; 64 * 1024];
                match self.socket.read(&mut chunk) {
                    Ok(0) => bail!(
                        "Grok leader closed; delivery may have succeeded; inspect before retrying"
                    ),
                    Ok(size) => self.bytes.extend_from_slice(&chunk[..size]),
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock
                                | std::io::ErrorKind::TimedOut
                                | std::io::ErrorKind::Interrupted
                        ) =>
                    {
                        return Ok(None);
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            #[cfg(not(unix))]
            {
                bail!("Grok native leader delivery requires Unix")
            }
        }
    }
    fn response(&mut self, expected: u64) -> Result<Option<Value>> {
        // Bound work per poll so a noisy vendor cannot starve DB presence renewal.
        for _ in 0..64 {
            let Some(frame) = self.frame()? else {
                return Ok(None);
            };
            match frame["type"].as_str() {
                Some("acp") => {
                    let rpc: Value = serde_json::from_str(
                        frame["payload"]
                            .as_str()
                            .ok_or_else(|| anyhow!("Grok ACP payload must be text"))?,
                    )?;
                    if rpc.get("method").is_some() && rpc.get("id").is_some() {
                        let mut refusal = crate::wire::refusal(&rpc["id"]);
                        refusal["jsonrpc"] = json!("2.0");
                        self.send_rpc(refusal)?;
                    } else if rpc["id"] == expected {
                        if rpc.get("error").is_some() {
                            bail!("Grok ACP request failed: {}", rpc["error"]);
                        }
                        return rpc
                            .get("result")
                            .cloned()
                            .map(Some)
                            .ok_or_else(|| anyhow!("Grok ACP response lacks result"));
                    }
                }
                Some("pong") => {}
                Some("error" | "shutdown" | "shutting_down") => bail!(
                    "Grok leader stopped; delivery may have succeeded; inspect before retrying"
                ),
                _ => bail!("Unsupported Grok leader frame"),
            }
        }
        Ok(None)
    }
    fn request(&mut self, method: &str, params: Value, deadline: Instant) -> Result<Value> {
        self.sequence += 1;
        let id = self.sequence;
        self.send_rpc(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        loop {
            if let Some(result) = self.response(id)? {
                return Ok(result);
            }
            if Instant::now() >= deadline {
                bail!("Grok {method} timed out");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    pub fn submit(&mut self, content: &str, token: &str) -> Result<()> {
        if self.pending.is_some() {
            bail!("Grok already has an in-flight delivery");
        }
        self.sequence += 1;
        let id = self.sequence;
        // Mark locally before the write: partial writes must never be retried implicitly.
        self.pending = Some((id, Instant::now() + TURN_TIMEOUT));
        self.send_rpc(json!({"jsonrpc":"2.0","id":id,"method":"session/prompt","params":{"sessionId":self.session,"prompt":[{"type":"text","text":content}],"_meta":{"verbatim":true,"promptId":token}}}))
    }
    /// Completion is a native receipt, not proof every communication message was handled.
    pub fn poll(&mut self) -> Result<Option<Value>> {
        let Some((id, deadline)) = self.pending else {
            return Ok(None);
        };
        if let Some(result) = self.response(id)? {
            if !result["stopReason"].is_string() {
                bail!("Grok prompt response lacks stopReason");
            }
            self.pending = None;
            return Ok(Some(result));
        }
        if Instant::now() >= deadline {
            bail!("Grok prompt timed out; delivery may have succeeded; inspect before retrying");
        }
        Ok(None)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn pair() -> Result<(Grok, std::os::unix::net::UnixStream)> {
        let (socket, peer) = std::os::unix::net::UnixStream::pair()?;
        socket.set_read_timeout(Some(Duration::from_millis(1)))?;
        socket.set_write_timeout(Some(Duration::from_secs(1)))?;
        Ok((
            Grok {
                socket,
                session: uuid::Uuid::new_v4().to_string(),
                bytes: Vec::new(),
                sequence: 0,
                pending: None,
            },
            peer,
        ))
    }
    fn encoded(value: Value) -> Vec<u8> {
        let bytes = value.to_string().into_bytes();
        [(bytes.len() as u32).to_be_bytes().as_slice(), &bytes].concat()
    }
    #[test]
    fn fragmented_frame_survives_idle_polls() -> Result<()> {
        let (mut client, mut peer) = pair()?;
        let bytes = encoded(json!({"type":"pong"}));
        for byte in &bytes[..bytes.len() - 1] {
            peer.write_all(&[*byte])?;
            assert!(client.frame()?.is_none());
            assert!(client.frame()?.is_none());
        }
        peer.write_all(&bytes[bytes.len() - 1..])?;
        assert_eq!(client.frame()?, Some(json!({"type":"pong"})));
        Ok(())
    }
    #[test]
    fn large_frame_arrives_in_one_poll() -> Result<()> {
        let (mut client, peer) = pair()?;
        let body = "x".repeat(200 * 1024);
        let bytes = encoded(json!({"type":"pong","body":body}));
        let writer = std::thread::spawn(move || {
            let mut peer = peer;
            peer.write_all(&bytes).map(|()| peer)
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut polls = 0;
        let frame = loop {
            polls += 1;
            if let Some(frame) = client.frame()? {
                break frame;
            }
            if Instant::now() >= deadline {
                bail!("Frame never completed");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(frame["body"].as_str().map(str::len), Some(200 * 1024));
        assert!(
            polls < 10,
            "a buffered frame must not need one poll per 8 KiB: {polls}"
        );
        writer.join().map_err(|_| anyhow!("writer panicked"))??;
        Ok(())
    }
    #[test]
    fn oversized_header_rejected_without_body_allocation() -> Result<()> {
        let (mut client, mut peer) = pair()?;
        peer.write_all(&((MAX_FRAME + 1) as u32).to_be_bytes())?;
        assert!(client.frame().is_err());
        assert_eq!(client.bytes.len(), 4);
        Ok(())
    }
    #[test]
    fn completion_is_separate_from_submission_and_never_replayed() -> Result<()> {
        let (mut client, mut peer) = pair()?;
        client.submit("new peer message", "dispatch-1")?;
        assert!(client.submit("duplicate", "dispatch-1").is_err());
        assert!(client.poll()?.is_none());
        let response = json!({"jsonrpc":"2.0","id":1,"result":{"stopReason":"end_turn"}});
        peer.write_all(&encoded(
            json!({"type":"acp","payload":response.to_string()}),
        ))?;
        assert_eq!(client.poll()?, Some(json!({"stopReason":"end_turn"})));
        assert!(client.poll()?.is_none());
        Ok(())
    }
    #[test]
    fn lost_connection_preserves_uncertain_inflight_state() -> Result<()> {
        let (mut client, peer) = pair()?;
        client.submit("new message", "dispatch-1")?;
        drop(peer);
        assert!(client.poll().is_err());
        assert!(client.submit("replay", "dispatch-1").is_err());
        Ok(())
    }
    #[test]
    fn reverse_requests_cannot_grant_permission() -> Result<()> {
        let (mut client, mut peer) = pair()?;
        peer.set_read_timeout(Some(Duration::from_secs(1)))?;
        let request = json!({"jsonrpc":"2.0","id":"approval","method":"session/request_permission","params":{}});
        peer.write_all(&encoded(
            json!({"type":"acp","payload":request.to_string()}),
        ))?;
        assert!(client.response(9)?.is_none());
        let mut len = [0; 4];
        peer.read_exact(&mut len)?;
        let mut body = vec![0; u32::from_be_bytes(len) as usize];
        peer.read_exact(&mut body)?;
        let frame: Value = serde_json::from_slice(&body)?;
        let rpc: Value = serde_json::from_str(
            frame["payload"]
                .as_str()
                .ok_or_else(|| anyhow!("payload"))?,
        )?;
        assert_eq!(rpc["error"]["code"], -32601);
        assert!(rpc.get("result").is_none());
        Ok(())
    }
}
