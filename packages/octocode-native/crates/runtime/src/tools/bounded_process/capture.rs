//! Capped and record-delimited reads of a command's output pipes.
use super::STDERR_BYTES;
use std::io::Read;

/// What a capped read kept, whether more arrived, and why it stopped early.
struct Capped {
    bytes: Vec<u8>,
    exceeded: bool,
    failure: Option<String>,
}

/// Reads to EOF or the first read error, keeping at most `cap` bytes.
fn read_to_cap(source: Option<impl Read>, cap: usize) -> Capped {
    let mut capped = Capped {
        bytes: Vec::new(),
        exceeded: false,
        failure: None,
    };
    let Some(mut source) = source else {
        return capped;
    };
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        match source.read(&mut chunk) {
            Ok(0) => return capped,
            Ok(read) => {
                let room = cap.saturating_sub(capped.bytes.len());
                capped.bytes.extend_from_slice(&chunk[..read.min(room)]);
                capped.exceeded |= read > room;
            }
            Err(error) => {
                capped.failure = Some(error.to_string());
                return capped;
            }
        }
    }
}

/// Reads to EOF, keeping at most `cap` bytes; `true` when more arrived. A
/// read error fails the read: the caller has no use for partial output.
pub(super) fn read_capped(
    source: Option<impl Read>,
    cap: usize,
) -> Result<(Vec<u8>, bool), String> {
    let capped = read_to_cap(source, cap);
    match capped.failure {
        Some(error) => Err(error),
        None => Ok((capped.bytes, capped.exceeded)),
    }
}

/// Stderr only quotes an error message, so what was read stands whether or
/// not the pipe reached its end; a cut says so in the text itself.
pub(super) fn read_stderr(source: Option<impl Read>) -> Vec<u8> {
    with_cut_note(read_to_cap(source, STDERR_BYTES))
}

/// Appends a closing `[stderr …]` line for each way `capped` fell short, so no
/// message that quotes it hides a cut.
fn with_cut_note(capped: Capped) -> Vec<u8> {
    let Capped {
        mut bytes,
        exceeded,
        failure,
    } = capped;
    let notes = [
        exceeded.then(|| format!("[stderr truncated at {STDERR_BYTES} bytes]")),
        failure.map(|error| format!("[stderr cut: {error}]")),
    ];
    for note in notes.into_iter().flatten() {
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        bytes.extend_from_slice(note.as_bytes());
    }
    bytes
}

pub(super) fn read_nul_records(
    source: Option<impl Read>,
    cap: usize,
    record: &mut (dyn FnMut(&[u8]) -> Result<(), String> + Send),
) -> Result<(), String> {
    let Some(mut source) = source else {
        return Ok(());
    };
    let mut pending = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let count = source.read(&mut chunk).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        for byte in &chunk[..count] {
            if *byte == 0 {
                record(&pending)?;
                pending.clear();
            } else {
                pending.push(*byte);
                if pending.len() > cap {
                    return Err(format!("record exceeded the {cap} byte output limit"));
                }
            }
        }
    }
    if !pending.is_empty() {
        return Err("unterminated NUL-delimited output record".into());
    }
    Ok(())
}
