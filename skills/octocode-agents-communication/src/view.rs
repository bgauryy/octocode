//! Loopback-only, token-scoped, read-only human dashboard. No models or new DB state.
use crate::{
    catalog,
    cli::{Args, output},
    database,
    store::Store,
    view_data,
};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn respond(stream: &mut TcpStream, status: &str, mime: &str, body: &[u8]) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}

fn decode_query(value: &str) -> Result<String> {
    let mut decoded = Vec::with_capacity(value.len());
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        decoded.push(match byte {
            b'+' => b' ',
            b'%' => {
                let high = bytes.next().and_then(|b| (b as char).to_digit(16));
                let low = bytes.next().and_then(|b| (b as char).to_digit(16));
                match (high, low) {
                    (Some(h), Some(l)) => (h * 16 + l) as u8,
                    _ => bail!("Invalid percent encoding"),
                }
            }
            other => other,
        });
    }
    Ok(String::from_utf8(decoded)?)
}

fn request(stream: &mut TcpStream, store: &Store, host: &str, prefix: &str) -> Result<()> {
    // BSD sockets inherit the nonblocking listener flag; request I/O uses bounded waits.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::new();
    let mut buffer = [0; 1024];
    while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
        if bytes.len() >= 8192 || Instant::now() > deadline {
            bail!("Request header limit");
        }
        let n = stream.read(&mut buffer)?;
        if n == 0 {
            return Ok(());
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    let text = std::str::from_utf8(&bytes)?;
    let mut lines = text.split("\r\n");
    let first = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>();
    if first.len() != 3 || first[0] != "GET" {
        return respond(
            stream,
            "405 Method Not Allowed",
            "text/plain",
            b"Read-only view: GET only",
        );
    }
    let headers = lines
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim()))
        .collect::<Vec<_>>();
    let hosts = headers
        .iter()
        .filter(|(k, _)| k == "host")
        .collect::<Vec<_>>();
    if hosts.len() != 1
        || hosts[0].1 != host
        || headers
            .iter()
            .any(|(k, v)| k == "origin" && *v != format!("http://{host}"))
    {
        return respond(
            stream,
            "403 Forbidden",
            "text/plain",
            b"Local origin required",
        );
    }
    let Some(route) = first[1].strip_prefix(prefix) else {
        return respond(stream, "404 Not Found", "text/plain", b"Not found");
    };
    match route {
        "" => respond(
            stream,
            "200 OK",
            "text/html; charset=utf-8",
            include_bytes!("view/index.html"),
        ),
        "app.js" => respond(
            stream,
            "200 OK",
            "text/javascript; charset=utf-8",
            include_bytes!("view/app.js"),
        ),
        "style.css" => respond(
            stream,
            "200 OK",
            "text/css; charset=utf-8",
            include_bytes!("view/style.css"),
        ),
        _ => {
            let result = if route == "api/summary" {
                view_data::summary(store)
            } else {
                (|| -> Result<Value> {
                    let (path, query) = route.split_once('?').unwrap_or((route, ""));
                    let entity = path
                        .strip_prefix("api/")
                        .ok_or_else(|| anyhow::anyhow!("Unknown route"))?;
                    let mut after = None;
                    let mut agent = None;
                    let mut filters = json!({});
                    for part in query.split('&').filter(|s| !s.is_empty()) {
                        let (key, value) = part
                            .split_once('=')
                            .ok_or_else(|| anyhow::anyhow!("Invalid query"))?;
                        // Cursor/agent are URL-safe; text filters are decoded and bound as SQL values.
                        match key {
                            "after" if after.is_none() => after = Some(value),
                            "agent" if agent.is_none() => agent = Some(value),
                            "q" | "conversation" | "status" if filters.get(key).is_none() => {
                                filters[key] = json!(decode_query(value)?);
                            }
                            _ => bail!("Unknown or duplicate filter"),
                        }
                    }
                    view_data::page(store, entity, after, agent, &filters)
                })()
            };
            match result {
                Ok(value) => respond(
                    stream,
                    "200 OK",
                    "application/json",
                    &serde_json::to_vec(&value)?,
                ),
                Err(error) => respond(
                    stream,
                    "400 Bad Request",
                    "application/json",
                    &serde_json::to_vec(&json!({"error":error.to_string()}))?,
                ),
            }
        }
    }
}

pub fn run(args: &Args, input: &Value) -> Result<()> {
    catalog::command("view", input)?;
    let store = Store::open(
        database::path(args.database.as_deref())?,
        &args.workspace,
        true,
        false,
    )?;
    let listener = TcpListener::bind(("127.0.0.1", input["port"].as_u64().unwrap_or(0) as u16))?;
    listener.set_nonblocking(true)?;
    let host = listener.local_addr()?.to_string();
    let prefix = format!("/{}/", uuid::Uuid::new_v4());
    let url = format!("http://{host}{prefix}");
    let stop = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&stop);
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;
    output(
        &json!({"url":url,"pid":std::process::id(),"workspace":store.workspace,"database":store.database,"readOnly":true,"stop":"Ctrl+C"}),
    )?;
    if input["open"].as_bool().unwrap_or(true) {
        let mut command = if cfg!(target_os = "macos") {
            Command::new("open")
        } else if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.args(["/C", "start", ""]);
            c
        } else {
            Command::new("xdg-open")
        };
        // Some desktop launchers remain alive until the browser closes.
        // Serving requests must never wait for that process.
        std::thread::spawn(move || {
            match command
                .arg(&url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
            {
                Ok(status) if status.success() => {}
                _ => eprintln!("Browser could not open; use the printed local URL"),
            }
        });
    }
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Err(error) = request(&mut stream, &store, &host, &prefix) {
                    eprintln!("View request ended: {error}");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25))
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
