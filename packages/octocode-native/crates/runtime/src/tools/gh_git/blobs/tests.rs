use super::frames::{Frame, FrameParser};

const OID: &str = "0123456789abcdef0123456789abcdef01234567";

fn parse(limit: usize, pieces: &[&[u8]]) -> Result<Vec<Frame>, String> {
    let mut parser = FrameParser::new(limit);
    let mut frames = Vec::new();
    for piece in pieces {
        parser.push(piece, &mut |frame| frames.push(frame))?;
    }
    parser.finish()?;
    Ok(frames)
}

fn blob(bytes: &[u8]) -> Frame {
    Frame::Blob {
        oid: OID.into(),
        bytes: bytes.to_vec(),
    }
}

#[test]
fn a_body_is_read_whole_whatever_the_chunk_boundaries() {
    let stream = format!("{OID} blob 11\nhello\nworld\n").into_bytes();
    let whole = parse(64, &[&stream]).unwrap();
    assert_eq!(whole, [blob(b"hello\nworld")]);
    // Every split point, including inside the header and between the body
    // and the newline that closes it.
    for at in 0..=stream.len() {
        let (head, tail) = stream.split_at(at);
        assert_eq!(parse(64, &[head, tail]).unwrap(), whole, "split at {at}");
    }
    // One byte at a time.
    let bytes: Vec<&[u8]> = stream.chunks(1).collect();
    assert_eq!(parse(64, &bytes).unwrap(), whole);
}

#[test]
fn an_empty_blob_a_missing_object_and_a_run_of_objects() {
    let stream = format!("{OID} blob 0\n\n{OID} missing\n{OID} blob 2\n\n\n").into_bytes();
    assert_eq!(
        parse(64, &[&stream]).unwrap(),
        [blob(b""), Frame::Missing { oid: OID.into() }, blob(b"\n\n")]
    );
}

#[test]
fn a_blob_past_the_limit_is_named_by_size_and_its_bytes_are_dropped() {
    let stream = format!("{OID} blob 10\n0123456789\n{OID} blob 3\nabc\n").into_bytes();
    assert_eq!(
        parse(5, &[&stream]).unwrap(),
        [
            Frame::Oversized {
                oid: OID.into(),
                size: 10
            },
            blob(b"abc")
        ]
    );
}

#[test]
fn output_that_is_cut_or_unexpected_is_a_failure_not_a_smaller_blob() {
    let cut = format!("{OID} blob 10\n01234").into_bytes();
    assert!(parse(64, &[&cut]).is_err(), "ends inside a body");
    assert!(
        parse(64, &[OID.as_bytes()]).is_err(),
        "ends inside a header"
    );
    let tree = format!("{OID} tree 1\nx\n").into_bytes();
    assert!(parse(64, &[&tree]).is_err(), "only blobs are asked for");
    let ragged = format!("{OID} blob 1\nxy").into_bytes();
    assert!(parse(64, &[&ragged]).is_err(), "a body ends in a newline");
}
