use super::super::DRAIN_GRACE;
use super::*;
use std::{io::Write, thread};

fn gated(finished: Option<Instant>) -> (Gated<std::io::PipeReader>, std::io::PipeWriter) {
    let (reader, writer) = std::io::pipe().expect("pipe");
    let cell = Arc::new(OnceLock::new());
    if let Some(end) = finished {
        cell.set(end).expect("fresh cell");
    }
    (Gated::new(reader, cell, Duration::ZERO), writer)
}

#[test]
fn a_gated_read_drains_pending_bytes_then_ends_while_the_writer_stays_open() {
    let (mut reader, mut writer) = gated(Some(Instant::now() + DRAIN_GRACE));
    writer.write_all(b"pending").expect("write");
    let mut chunk = [0_u8; 64];
    let count = reader.read(&mut chunk).expect("pending bytes");
    assert_eq!(&chunk[..count], b"pending");
    assert_eq!(reader.read(&mut chunk).expect("quiet pipe"), 0);
    drop(writer);
}

/// Bytes already in the pipe at the end are the command's own output:
/// read even past the grace. Bytes that arrive after them are not.
#[test]
fn a_gated_read_past_the_grace_drains_what_was_pending_then_fails_on_more() {
    let (mut reader, mut writer) = gated(Some(Instant::now()));
    writer.write_all(b"pending").expect("write");
    let mut chunk = [0_u8; 64];
    let count = reader.read(&mut chunk).expect("pending bytes");
    assert_eq!(&chunk[..count], b"pending");
    writer.write_all(b"still arriving").expect("write");
    let error = reader.read(&mut chunk).expect_err("past the grace");
    assert!(error.to_string().contains("kept streaming"), "{error}");
}

#[test]
fn a_gated_read_past_the_grace_reads_a_closed_pipe_to_its_end() {
    let (mut reader, mut writer) = gated(Some(Instant::now()));
    writer.write_all(b"done").expect("write");
    drop(writer);
    let mut chunk = [0_u8; 64];
    let count = reader.read(&mut chunk).expect("pending bytes");
    assert_eq!(&chunk[..count], b"done");
    // A child another test thread forks may hold a copy of the write end
    // until it execs: wait until the end is in the pipe, as an exited
    // command's is.
    assert!(
        poll_readable(
            std::os::fd::AsRawFd::as_raw_fd(&reader.inner),
            Duration::from_secs(5)
        )
        .expect("poll")
    );
    assert_eq!(reader.read(&mut chunk).expect("closed pipe"), 0);
}

/// Past the grace, a pipe a descendant still holds open fails the read
/// even when it is quiet: the output may not be complete.
#[test]
fn a_gated_read_past_the_grace_fails_on_a_quiet_pipe_still_held_open() {
    let (mut reader, writer) = gated(Some(Instant::now()));
    let error = reader.read(&mut [0_u8; 64]).expect_err("past the grace");
    assert!(error.to_string().contains("kept streaming"), "{error}");
    drop(writer);
}

#[test]
fn a_gated_read_before_the_end_blocks_for_data_not_for_the_writer() {
    let (mut reader, writer) = gated(None);
    let late = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        let mut writer = writer;
        writer.write_all(b"late").expect("write");
    });
    let mut chunk = [0_u8; 64];
    let count = reader.read(&mut chunk).expect("data arrives");
    assert_eq!(&chunk[..count], b"late");
    late.join().expect("writer thread");
    assert_eq!(reader.read(&mut chunk).expect("closed pipe"), 0);
}
