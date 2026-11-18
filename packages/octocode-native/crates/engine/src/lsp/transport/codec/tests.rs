use super::*;
use serde_json::json;
use tokio::io::{AsyncWriteExt, duplex};

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

async fn frames_from(bytes: &[u8]) -> Vec<std::result::Result<Frame, FrameError>> {
    let mut reader = BufReader::new(bytes);
    let mut out = Vec::new();
    loop {
        let frame = read_frame(&mut reader).await;
        let stop = !matches!(frame, Ok(Frame::Body(_) | Frame::Empty));
        out.push(frame);
        if stop {
            return out;
        }
    }
}

#[test]
fn frame_split_at_every_byte_decodes_identically() {
    block_on(async {
        let messages = [
            json!({"jsonrpc":"2.0","id":1,"result":{"é":"ü😀"}}),
            json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":"t"}}),
        ];
        let mut stream = Vec::new();
        for message in &messages {
            stream.extend(encode_frame(message).expect("encode"));
        }
        for split in 0..=stream.len() {
            // `chain` yields the first slice fully before the second, so the
            // reader sees the stream cut at `split`.
            let chained = (&stream[..split]).chain(&stream[split..]);
            let mut reader = BufReader::with_capacity(7, chained);
            for expected in &messages {
                let Ok(Frame::Body(body)) = read_frame(&mut reader).await else {
                    panic!("split {split}: expected a body frame");
                };
                let decoded: Value = serde_json::from_slice(&body).expect("json");
                assert_eq!(&decoded, expected, "split at {split}");
            }
            assert!(matches!(read_frame(&mut reader).await, Ok(Frame::Eof)));
        }
    });
}

#[test]
fn content_length_header_is_case_insensitive_and_extra_headers_ignored() {
    block_on(async {
        let frames =
            frames_from(b"content-length: 2\r\nContent-Type: application/vscode-jsonrpc\r\n\r\n{}")
                .await;
        assert!(matches!(&frames[0], Ok(Frame::Body(body)) if body == b"{}"));
    });
}

#[test]
fn missing_content_length_is_an_empty_frame_not_eof() {
    block_on(async {
        let frames =
            frames_from(b"X-Unknown: whatever\r\n\r\n\r\nContent-Length: 2\r\n\r\n{}").await;
        assert!(matches!(frames[0], Ok(Frame::Empty)));
        assert!(matches!(frames[1], Ok(Frame::Empty)));
        assert!(matches!(&frames[2], Ok(Frame::Body(body)) if body == b"{}"));
        assert!(matches!(frames[3], Ok(Frame::Eof)));
    });
}

#[test]
fn garbage_content_length_value_is_fatal() {
    block_on(async {
        for value in ["abc", "-1", "+2", "1 2", "", "99999999999999999999999"] {
            let input = format!("Content-Length: {value}\r\n\r\n{{}}");
            let frames = frames_from(input.as_bytes()).await;
            let Err(error) = &frames[0] else {
                panic!(
                    "Content-Length {value:?} must be fatal, got {:?}",
                    frames[0]
                );
            };
            assert!(error.0.contains("invalid Content-Length"), "{error}");
        }
    });
}

#[test]
fn oversized_content_length_is_rejected_before_allocation() {
    block_on(async {
        let input = format!("Content-Length: {}\r\n\r\n", MAX_CONTENT_LENGTH + 1);
        let frames = frames_from(input.as_bytes()).await;
        assert!(
            matches!(&frames[0], Err(error) if error.0.contains("maximum JSON-RPC frame size"))
        );
    });
}

#[test]
fn truncated_body_or_header_is_fatal_not_eof() {
    block_on(async {
        let frames = frames_from(b"Content-Length: 10\r\n\r\n{}").await;
        assert!(matches!(&frames[0], Err(error) if error.0.contains("mid-frame")));
        let frames = frames_from(b"Content-Length: 10\r\n").await;
        assert!(matches!(&frames[0], Err(error) if error.0.contains("mid-header")));
        assert!(matches!(frames_from(b"").await[0], Ok(Frame::Eof)));
    });
}

#[test]
fn unbounded_header_line_is_rejected() {
    block_on(async {
        let (mut server, client_reader) = duplex(64 * 1024);
        let writer = tokio::spawn(async move {
            let _ = server.write_all(&vec![b'A'; 16 * 1024]).await;
            // Keep the stream open so the reader hits the cap, not EOF.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        });
        let mut reader = BufReader::new(client_reader);
        let outcome = read_frame(&mut reader).await;
        assert!(matches!(outcome, Err(error) if error.0.contains("header line")));
        let _ = writer.await;
    });
}

#[test]
fn encode_frame_writes_header_and_body_in_one_buffer() {
    let frame = encode_frame(&json!({"a":"é"})).expect("encode");
    let text = String::from_utf8(frame).expect("utf8");
    // "é" is two UTF-8 bytes: Content-Length counts bytes, not chars.
    assert_eq!(text, "Content-Length: 10\r\n\r\n{\"a\":\"é\"}");
}
