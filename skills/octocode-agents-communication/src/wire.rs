use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// One frame bound for every stdio, socket and IPC transport.
pub(crate) const MAX_FRAME: usize = 8 * 1024 * 1024;
/// A peer sent a frame above [`MAX_FRAME`]; its remainder is still unread.
#[derive(Debug)]
pub(crate) struct OversizedFrame;
impl std::fmt::Display for OversizedFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JSON frame exceeds 8 MiB")
    }
}
impl std::error::Error for OversizedFrame {}
pub(crate) fn read_frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_FRAME + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if bytes.len() > MAX_FRAME {
        return Err(OversizedFrame.into());
    }
    Ok(if bytes.is_empty() { None } else { Some(bytes) })
}
/// Discard the rest of an oversized line without buffering it, so a server can
/// reject that one request and keep serving the ones queued behind it.
pub(crate) fn skip_line(reader: &mut impl BufRead) -> Result<()> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(());
        }
        if let Some(end) = buffer.iter().position(|b| *b == b'\n') {
            reader.consume(end + 1);
            return Ok(());
        }
        let size = buffer.len();
        reader.consume(size);
    }
}
/// The single reply to vendor reverse requests: delivery never grants approvals or runs tools.
pub(crate) fn refusal(id: &Value) -> Value {
    json!({"id":id,"error":{"code":-32601,"message":"Communication delivery cannot approve actions or execute tools"}})
}
pub(crate) struct Wire {
    child: Child,
    input: Option<SyncSender<Vec<u8>>>,
    written: Option<Receiver<Result<(), String>>>,
    writer: Option<JoinHandle<()>>,
    rx: Option<Receiver<Result<Value, String>>>,
    reader: Option<JoinHandle<()>>,
    sequence: u64,
    pending: VecDeque<Value>,
    stopped: bool,
    deadline: Option<Instant>,
    stop: Arc<AtomicBool>,
}
impl Wire {
    pub fn start(
        command: &str,
        args: &[String],
        cwd: &Path,
        environment: &[(&str, String)],
        deadline: Option<Instant>,
        stop: Arc<AtomicBool>,
    ) -> Result<Self> {
        let mut command = Command::new(command);
        command
            .args(args)
            .envs(environment.iter().map(|(key, value)| (key, value)))
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        // A killed worker must not orphan its vendor agent. Linux can tie the child to
        // this process; elsewhere close() and the proxy's parent watch own teardown.
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            // SAFETY: prctl is async-signal-safe and touches no parent state.
            unsafe {
                command.pre_exec(|| {
                    nix::sys::prctl::set_pdeathsig(nix::sys::signal::Signal::SIGKILL)
                        .map_err(std::io::Error::from)
                });
            }
        }
        let mut child = command.spawn()?;
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Missing child stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Missing child stdout"))?;
        let (write_tx, write_rx) = mpsc::sync_channel::<Vec<u8>>(1);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let writer = thread::spawn(move || {
            while let Ok(frame) = write_rx.recv() {
                let result = input
                    .write_all(&frame)
                    .and_then(|()| input.flush())
                    .map_err(|error| error.to_string());
                let failed = result.is_err();
                if done_tx.send(result).is_err() || failed {
                    break;
                }
            }
        });
        let (tx, rx) = mpsc::sync_channel(128);
        let reader = thread::spawn(move || {
            let mut input = BufReader::new(stdout);
            loop {
                let event = match read_frame(&mut input) {
                    Ok(Some(line)) => {
                        serde_json::from_slice::<Value>(&line).map_err(|e| e.to_string())
                    }
                    Ok(None) => Err("Vendor process closed stdout".to_owned()),
                    Err(error) => Err(error.to_string()),
                };
                let failed = event.is_err();
                if tx.send(event).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input: Some(write_tx),
            written: Some(done_rx),
            writer: Some(writer),
            rx: Some(rx),
            reader: Some(reader),
            sequence: 0,
            pending: VecDeque::new(),
            stopped: false,
            deadline,
            stop,
        })
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
    pub fn send(&mut self, value: &Value) -> Result<()> {
        if self.stopped {
            bail!("Vendor process closed");
        }
        let mut bytes = serde_json::to_vec(value)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_FRAME {
            bail!("JSON frame exceeds 8 MiB");
        }
        self.input
            .as_ref()
            .ok_or_else(|| anyhow!("Process closed"))?
            .try_send(bytes)?;
        let timeout = Instant::now() + Duration::from_secs(30);
        let deadline = self.deadline.map_or(timeout, |d| d.min(timeout));
        loop {
            if self.stop.load(Ordering::Relaxed) {
                bail!("Worker stopped");
            }
            if Instant::now() >= deadline {
                bail!("Timed out writing to vendor");
            }
            match self
                .written
                .as_ref()
                .ok_or_else(|| anyhow!("Process closed"))?
                .recv_timeout(Duration::from_millis(100))
            {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(error)) => bail!("{error}"),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => bail!("Vendor input disconnected"),
            }
        }
    }
    fn receive(&self, timeout: Duration) -> Result<Option<Value>> {
        match self
            .rx
            .as_ref()
            .ok_or_else(|| anyhow!("Process closed"))?
            .recv_timeout(timeout)
        {
            Ok(Ok(event)) => Ok(Some(event)),
            Ok(Err(error)) => bail!("{error}"),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => bail!("Vendor process disconnected"),
        }
    }
    pub fn event(&mut self, timeout: Duration) -> Result<Option<Value>> {
        if let Some(value) = self.pending.pop_front() {
            return Ok(Some(value));
        }
        self.receive(timeout)
    }
    pub fn request(&mut self, method: &str, params: Value, stop: &AtomicBool) -> Result<Value> {
        self.exchange(method, params, stop, false)
    }
    pub fn pi_request(&mut self, method: &str, params: Value, stop: &AtomicBool) -> Result<Value> {
        self.exchange(method, params, stop, true)
    }
    fn exchange(
        &mut self,
        method: &str,
        params: Value,
        stop: &AtomicBool,
        pi: bool,
    ) -> Result<Value> {
        self.sequence += 1;
        let id = if pi {
            json!(self.sequence.to_string())
        } else {
            json!(self.sequence)
        };
        let message = if pi {
            let mut message = params;
            message["id"] = id.clone();
            message["type"] = json!(method);
            message
        } else {
            json!({"id":id,"method":method,"params":params})
        };
        self.send(&message)?;
        let timeout = Instant::now() + Duration::from_secs(30);
        let deadline = self.deadline.map_or(timeout, |d| d.min(timeout));
        loop {
            if stop.load(Ordering::Relaxed) {
                bail!("Worker stopped");
            }
            if Instant::now() >= deadline {
                bail!("Timed out: {method}");
            }
            if let Some(value) = self.receive(Duration::from_millis(100))? {
                if value["id"] == id && value.get("method").is_none() {
                    if let Some(error) = value.get("error") {
                        bail!("Vendor request failed: {error}");
                    }
                    if pi && value["success"] != true {
                        bail!("Vendor request failed: {value}");
                    }
                    return Ok(value[if pi { "data" } else { "result" }].clone());
                }
                if value.get("id").is_some() && value.get("method").is_some() {
                    self.send(&refusal(&value["id"]))?;
                } else {
                    if self.pending.len() >= 512 {
                        bail!("Vendor notification queue full");
                    }
                    self.pending.push_back(value);
                }
            }
        }
    }
    pub fn close(&mut self) -> Result<()> {
        if self.stopped {
            return Ok(());
        }
        self.stopped = true;
        self.input.take();
        self.written.take();
        #[cfg(unix)]
        {
            use nix::{
                sys::signal::{Signal, killpg},
                unistd::Pid,
            };
            let group = Pid::from_raw(self.child.id() as i32);
            let _ = killpg(group, Signal::SIGTERM);
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.child.try_wait()?.is_none() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
            // The unreaped leader pins its group ID. After a reap the ID may be reused,
            // so the group is escalated only while the leader is still unreaped.
            if self.child.try_wait()?.is_none() {
                let _ = killpg(group, Signal::SIGKILL);
            }
        }
        #[cfg(windows)]
        {
            // taskkill /T terminates only this owned process tree.
            let _ = Command::new("taskkill")
                .args(["/PID", &self.child.id().to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        self.child.wait()?;
        self.rx.take();
        // A grandchild that escaped the group can keep stdout open; never hang on it.
        let deadline = Instant::now() + Duration::from_secs(1);
        for handle in [self.reader.take(), self.writer.take()]
            .into_iter()
            .flatten()
        {
            while !handle.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
        Ok(())
    }
}
impl Drop for Wire {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
