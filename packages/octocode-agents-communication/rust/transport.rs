use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpStream},
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, client::client_with_config, protocol::WebSocketConfig};

pub mod grok;
pub mod opencode;
pub mod protocol;

/// Capabilities describe this adapter, not permissions or proof that a model read a message.
pub fn capabilities(mode: &str) -> Value {
    let Ok(transport) = protocol::NativeTransport::parse(mode) else {
        return json!({"nativeInput":false,"passiveInjection":null,"actionWake":null,"acceptanceReceipt":"host-confirmation","readReceipt":false});
    };
    let mut result = json!({"nativeInput":true,"passiveInjection":!transport.requires_action(),"actionWake":true,"acceptanceReceipt":transport.receipt().name(),"readReceipt":false});
    match transport {
        protocol::NativeTransport::Codex => {
            result["wakePrerequisite"] = json!("loaded-idle-thread")
        }
        protocol::NativeTransport::OpenCode => {
            result["wakePrerequisite"] = json!("existing-idle-session-in-workspace")
        }
        _ => {}
    }
    result
}

enum Stream {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
}
// Socket timeouts bound one syscall. The deadline also bounds a fragmented frame
// whose peer keeps every individual read alive indefinitely.
struct DeadlineStream {
    inner: Stream,
    deadline: Instant,
}
impl DeadlineStream {
    fn remaining(&self) -> std::io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "Codex I/O timed out"))
    }
}
impl Read for DeadlineStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let remaining = self.remaining()?;
        match &self.inner {
            Stream::Tcp(s) => s.set_read_timeout(Some(remaining))?,
            #[cfg(unix)]
            Stream::Unix(s) => s.set_read_timeout(Some(remaining))?,
        }
        self.inner.read(buf)
    }
}
impl Write for DeadlineStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let remaining = self.remaining()?;
        match &self.inner {
            Stream::Tcp(s) => s.set_write_timeout(Some(remaining))?,
            #[cfg(unix)]
            Stream::Unix(s) => s.set_write_timeout(Some(remaining))?,
        }
        self.inner.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.remaining()?;
        self.inner.flush()
    }
}
impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.read(buf),
            #[cfg(unix)]
            Self::Unix(s) => s.read(buf),
        }
    }
}
impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.write(buf),
            #[cfg(unix)]
            Self::Unix(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(s) => s.flush(),
            #[cfg(unix)]
            Self::Unix(s) => s.flush(),
        }
    }
}
pub fn validate(transport: &str, endpoint: Option<&str>) -> Result<()> {
    if transport == "raw" {
        if endpoint.is_some() {
            bail!("Raw hooks do not have an endpoint");
        }
        return Ok(());
    }
    let endpoint = endpoint.ok_or_else(|| anyhow!("Native attachment requires endpoint"))?;
    if transport == "grok" {
        return grok::validate(endpoint);
    }
    if transport == "opencode" {
        return opencode::validate(endpoint);
    }
    if !matches!(transport, "claude" | "codex") {
        bail!("Unknown native transport");
    }
    if transport == "claude" || endpoint.starts_with("unix://") {
        let path = endpoint.strip_prefix("unix://").unwrap_or(endpoint);
        if !std::path::Path::new(path).is_absolute() || path.len() > 103 {
            bail!("Unix socket endpoint must be an absolute path of at most 103 bytes");
        }
        return Ok(());
    }
    let _: SocketAddr = address(endpoint)?;
    Ok(())
}
fn address(endpoint: &str) -> Result<SocketAddr> {
    let uri: tungstenite::http::Uri = endpoint.parse()?;
    if uri.scheme_str() != Some("ws")
        || uri.query().is_some()
        || uri.path() != "/"
        || uri.authority().is_some_and(|a| a.as_str().contains('@'))
    {
        bail!(
            "Codex endpoint must be ws://loopback:port or unix:///absolute/socket; no credentials or query"
        );
    }
    let host = uri
        .host()
        .ok_or_else(|| anyhow!("Endpoint host required"))?;
    let ip: IpAddr = if host == "localhost" {
        "127.0.0.1".parse()?
    } else {
        host.trim_matches(['[', ']']).parse()?
    };
    if !ip.is_loopback() {
        bail!("Only local loopback endpoints are supported");
    }
    Ok(SocketAddr::new(
        ip,
        uri.port_u16()
            .ok_or_else(|| anyhow!("Endpoint port required"))?,
    ))
}
#[cfg(unix)]
fn unix(path: &str) -> Result<std::os::unix::net::UnixStream> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_socket() || metadata.uid() != nix::unistd::geteuid().as_raw() {
        bail!("Endpoint must be a socket owned by this OS user, not a symlink");
    }
    let stream = std::os::unix::net::UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    Ok(stream)
}

pub struct Codex {
    socket: WebSocket<DeadlineStream>,
    sequence: u64,
}
impl Codex {
    pub fn connect(endpoint: &str) -> Result<Self> {
        validate("codex", Some(endpoint))?;
        let (stream, url) = if let Some(path) = endpoint.strip_prefix("unix://") {
            #[cfg(unix)]
            {
                (Stream::Unix(unix(path)?), "ws://localhost/")
            }
            #[cfg(not(unix))]
            {
                let _ = path;
                bail!(
                    "Unix socket delivery is unavailable on this platform; use loopback WebSocket"
                );
            }
        } else {
            let stream = TcpStream::connect_timeout(&address(endpoint)?, Duration::from_secs(3))?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            stream.set_write_timeout(Some(Duration::from_secs(3)))?;
            (Stream::Tcp(stream), endpoint)
        };
        let stream = DeadlineStream {
            inner: stream,
            deadline: Instant::now() + Duration::from_secs(5),
        };
        let config = WebSocketConfig::default()
            .max_message_size(Some(1024 * 1024))
            .max_frame_size(Some(1024 * 1024));
        let (socket, _) =
            client_with_config(url, stream, Some(config)).map_err(|e| anyhow!(e.to_string()))?;
        let mut client = Self {
            socket,
            sequence: 0,
        };
        client.request("initialize", json!({"clientInfo":{"name":"octocode-communication-dispatch","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}))?;
        client.send(json!({"method":"initialized","params":{}}))?;
        Ok(client)
    }
    fn send(&mut self, value: Value) -> Result<()> {
        self.socket.send(Message::Text(value.to_string().into()))?;
        Ok(())
    }
    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.sequence += 1;
        let id = self.sequence;
        let deadline = Instant::now() + Duration::from_secs(5);
        self.socket.get_mut().deadline = deadline;
        self.send(json!({"id":id,"method":method,"params":params}))?;
        while Instant::now() < deadline {
            match self.socket.read() {
                Ok(Message::Text(text)) => {
                    let frame: Value = serde_json::from_str(&text)?;
                    if frame["id"] == id && frame.get("method").is_none() {
                        if !frame["error"].is_null() {
                            bail!("Codex {method}: {}", frame["error"]);
                        }
                        return Ok(frame["result"].clone());
                    }
                    if frame.get("id").is_some() && frame.get("method").is_some() {
                        self.send(json!({"id":frame["id"],"error":{"code":-32601,"message":"Delivery client cannot approve actions"}}))?;
                    }
                }
                Ok(Message::Close(_)) => bail!("Codex endpoint closed"),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(e.into()),
            }
        }
        bail!("Codex {method} timed out; delivery may have succeeded; inspect before retrying")
    }
    pub fn inject(&mut self, session: &str, content: &str) -> Result<()> {
        self.request("thread/inject_items",json!({"threadId":session,"items":[{"type":"message","role":"user","content":[{"type":"input_text","text":content}]}]}))?;
        Ok(())
    }
    pub fn idle(&mut self, session: &str, workspace: &str) -> Result<bool> {
        // Metadata only: never fetch recipient history or implicitly load a stopped host.
        let result = self.request(
            "thread/read",
            json!({"threadId":session,"includeTurns":false}),
        )?;
        if result["thread"]["id"] != session {
            bail!("Codex thread/read returned a different or missing thread identity");
        }
        let cwd = result["thread"]["cwd"]
            .as_str()
            .ok_or_else(|| anyhow!("Codex thread/read returned no workspace"))?;
        if std::fs::canonicalize(cwd)? != std::path::Path::new(workspace) {
            bail!("Codex thread/read returned a different workspace");
        }
        match result["thread"]["status"]["type"].as_str() {
            Some("idle") => Ok(true),
            Some("active" | "notLoaded" | "systemError") => Ok(false),
            _ => bail!("Codex thread/read returned an unknown runtime status"),
        }
    }
    pub fn start_turn(&mut self, session: &str, content: &str) -> Result<()> {
        // One input effect; never inject the same body first. Existing host settings remain intact.
        let result = self.request(
            "turn/start",
            json!({"threadId":session,"input":[{"type":"text","text":content}]}),
        )?;
        if result["turn"]["id"].as_str().is_none_or(str::is_empty)
            || !matches!(
                result["turn"]["status"].as_str(),
                Some("inProgress" | "completed" | "failed" | "interrupted")
            )
        {
            bail!("Codex turn/start returned no valid acceptance receipt; inspect before retrying");
        }
        Ok(())
    }
}
pub fn claude(endpoint: &str, session: &str, token: &str, content: &str) -> Result<()> {
    #[cfg(unix)]
    {
        let path = endpoint.strip_prefix("unix://").unwrap_or(endpoint);
        let mut stream = unix(path)?;
        // Only the owning hook's exported token is eligible; never discover/copy another session's credentials.
        if std::env::var("CLAUDE_CODE_MESSAGING_SOCKET")
            .ok()
            .as_deref()
            == Some(path)
            && let Ok(auth) = std::env::var("CLAUDE_CODE_MESSAGING_TOKEN")
        {
            writeln!(stream, "{}", json!({"type":"auth","token":auth}))?;
        }
        writeln!(
            stream,
            "{}",
            json!({"type":"user","session_id":session,"from":"octocode-communication","uuid":token,"msg_id":token,"message":{"role":"user","content":content}})
        )?;
        stream.shutdown(std::net::Shutdown::Write)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (endpoint, session, token, content);
        bail!("Claude native socket delivery requires Unix; use the raw hook on this platform")
    }
}
