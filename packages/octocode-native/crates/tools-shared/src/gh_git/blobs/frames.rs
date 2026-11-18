//! The framing of `git cat-file --batch`: `<oid> <type> <size> LF <bytes> LF`
//! per found object and `<oid> missing LF` per absent one, fed in pieces.

/// One answer of `cat-file --batch`.
#[derive(Debug, Eq, PartialEq)]
pub enum Frame {
    Blob {
        oid: String,
        bytes: Vec<u8>,
    },
    /// Longer than the limit: the size, with the bytes dropped as they arrived.
    Oversized {
        oid: String,
        size: u64,
    },
    Missing {
        oid: String,
    },
}

enum State {
    /// The header line so far.
    Header(Vec<u8>),
    Body {
        oid: String,
        size: u64,
        remaining: u64,
        /// `None` once the blob is known to exceed the limit.
        bytes: Option<Vec<u8>>,
    },
    /// The LF that closes a body.
    Terminator,
}

pub(super) struct FrameParser {
    limit: usize,
    state: State,
}

/// Longest header line git writes: an object id, a type and a size.
const HEADER_LIMIT: usize = 256;

impl FrameParser {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            state: State::Header(Vec::new()),
        }
    }

    /// Consume `data`, handing each completed frame to `frame`.
    pub(super) fn push(
        &mut self,
        mut data: &[u8],
        frame: &mut (dyn FnMut(Frame) + Send),
    ) -> Result<(), String> {
        while !data.is_empty() {
            match &mut self.state {
                State::Header(line) => {
                    let Some(end) = data.iter().position(|byte| *byte == b'\n') else {
                        line.extend_from_slice(data);
                        if line.len() > HEADER_LIMIT {
                            return Err("git cat-file wrote an oversized header line".into());
                        }
                        return Ok(());
                    };
                    line.extend_from_slice(&data[..end]);
                    data = &data[end + 1..];
                    let header = std::mem::take(line);
                    self.state = self.open(&header, frame)?;
                    // An empty body has no bytes to wait for.
                    if matches!(self.state, State::Body { remaining: 0, .. }) {
                        self.close(frame);
                    }
                }
                State::Body {
                    remaining, bytes, ..
                } => {
                    let take =
                        usize::try_from(*remaining).map_or(data.len(), |left| left.min(data.len()));
                    if let Some(bytes) = bytes {
                        bytes.extend_from_slice(&data[..take]);
                    }
                    *remaining -= take as u64;
                    data = &data[take..];
                    if *remaining == 0 {
                        self.close(frame);
                    }
                }
                State::Terminator => {
                    if data[0] != b'\n' {
                        return Err("git cat-file object is not followed by a newline".into());
                    }
                    data = &data[1..];
                    self.state = State::Header(Vec::new());
                }
            }
        }
        Ok(())
    }

    /// The header line's meaning: a missing object, or the body that follows.
    fn open(&self, header: &[u8], frame: &mut (dyn FnMut(Frame) + Send)) -> Result<State, String> {
        let line = String::from_utf8_lossy(header);
        let mut parts = line.split(' ');
        let oid = parts.next().unwrap_or_default().to_owned();
        match (parts.next(), parts.next(), parts.next()) {
            (Some("missing"), None, None) => {
                frame(Frame::Missing { oid });
                Ok(State::Header(Vec::new()))
            }
            (Some("blob"), Some(size), None) => {
                let size: u64 = size
                    .parse()
                    .map_err(|_| format!("git cat-file wrote an invalid size: {line}"))?;
                let keep = usize::try_from(size).is_ok_and(|size| size <= self.limit);
                Ok(State::Body {
                    oid,
                    size,
                    remaining: size,
                    bytes: keep.then(|| Vec::with_capacity(usize::try_from(size).unwrap_or(0))),
                })
            }
            _ => Err(format!("git cat-file wrote an unexpected answer: {line}")),
        }
    }

    /// The frame of a finished body, then the state that expects its LF.
    fn closed(state: State, frame: &mut (dyn FnMut(Frame) + Send)) -> State {
        if let State::Body {
            oid, size, bytes, ..
        } = state
        {
            frame(match bytes {
                Some(bytes) => Frame::Blob { oid, bytes },
                None => Frame::Oversized { oid, size },
            });
        }
        State::Terminator
    }

    fn close(&mut self, frame: &mut (dyn FnMut(Frame) + Send)) {
        let state = std::mem::replace(&mut self.state, State::Terminator);
        self.state = Self::closed(state, frame);
    }

    /// The output ended: an object cut short is a failure, not a smaller blob.
    pub(super) fn finish(&self) -> Result<(), String> {
        match &self.state {
            State::Header(line) if line.is_empty() => Ok(()),
            // A final object whose closing LF was never read is whole.
            State::Terminator => Ok(()),
            _ => Err("git cat-file output ended inside an object".into()),
        }
    }
}
