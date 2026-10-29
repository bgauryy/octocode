//! Admitted sections are judged concurrently, proven by a provider that holds requests until a quorum arrives.

use super::sections;
use crate::support::{PROVIDER_LIMITS, Workspace};
use serde_json::{Value, json};
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// A provider that holds every request until `quorum` requests are on the
/// wire at once (or `patience` passes), then answers each question Noul 0.9.
/// `peak` is the most requests it ever held together.
struct Rendezvous {
    uri: String,
    state: Arc<(Mutex<RendezvousState>, Condvar)>,
}

#[derive(Default)]
struct RendezvousState {
    in_flight: usize,
    peak: usize,
}

impl Rendezvous {
    fn start(quorum: usize, patience: Duration) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind provider");
        let uri = format!(
            "http://{}",
            listener.local_addr().expect("provider address")
        );
        let state = Arc::new((Mutex::new(RendezvousState::default()), Condvar::new()));
        let shared = Arc::clone(&state);
        std::thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                let state = Arc::clone(&shared);
                std::thread::spawn(move || Self::answer(stream, &state, quorum, patience));
            }
        });
        Self { uri, state }
    }

    fn answer(
        stream: std::net::TcpStream,
        state: &(Mutex<RendezvousState>, Condvar),
        quorum: usize,
        patience: Duration,
    ) {
        let mut reader = BufReader::new(stream);
        let mut length = 0;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            if line == "\r\n" {
                break;
            }
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("content-length")
            {
                length = value.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        let (lock, arrived) = state;
        let mut held = lock.lock().expect("provider state");
        held.in_flight += 1;
        held.peak = held.peak.max(held.in_flight);
        arrived.notify_all();
        let (mut held, _) = arrived
            .wait_timeout_while(held, patience, |held| held.peak < quorum)
            .expect("provider state");
        held.in_flight -= 1;
        drop(held);
        let request: Value = serde_json::from_slice(&body).expect("provider body");
        let answers = request["questions"]
            .as_object()
            .expect("questions")
            .keys()
            .map(|id| (id.clone(), json!({"type":"noul","noul":0.9})))
            .collect::<serde_json::Map<_, _>>();
        let response = json!({"model":"jev-1.13.0","answers":answers,
            "usage":{"input_tokens":10,"output_tokens":2}})
        .to_string();
        let mut stream = reader.into_inner();
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{response}",
            response.len()
        );
    }

    fn peak(&self) -> usize {
        self.state.0.lock().expect("provider state").peak
    }
}

#[tokio::test]
async fn admitted_sections_are_judged_concurrently_and_all_answer() {
    let sections_count = 4;
    let provider = Rendezvous::start(sections_count, Duration::from_secs(3));
    let workspace = Workspace::new();
    workspace.write("four.rs", super::separate_functions(sections_count));
    let runtime = workspace.runtime_with(
        &[
            ("OCTOCODE_BETA", "true".to_owned()),
            ("OCTOCODE_CLASSIFICATION_API", "secret".to_owned()),
            ("OCTOCODE_CLASSIFICATION_API_HOST", provider.uri.clone()),
        ],
        PROVIDER_LIMITS,
    );
    let out = sections(
        &runtime,
        json!({"file":{"tool":"localFetch","query":{"path":"four.rs"}}}),
    )
    .await;
    let file = &out["resources"]["file"];
    assert_eq!(file["coverage"]["totalSections"], sections_count, "{out:#}");
    assert_eq!(file["coverage"]["answered"], sections_count, "{out:#}");
    assert_eq!(
        provider.peak(),
        sections_count,
        "every admitted section is on the wire at once"
    );
    assert!(out.get("next").is_none(), "{out:#}");
    runtime.close().await;
}
