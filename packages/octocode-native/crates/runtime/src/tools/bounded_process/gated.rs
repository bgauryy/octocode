//! The output pipe gate: reads that stop at the end of the command.
#[cfg(unix)]
use super::POLL;
use std::time::Instant;
use std::{
    io::Read,
    sync::{Arc, OnceLock},
    time::Duration,
};

/// A command's output pipe whose reads stop at the end of the command.
///
/// On Unix a read polls the descriptor, so it wakes to see that the command
/// ended. Once it did, a read first drains the bytes already in the pipe
/// when it saw the end (the command's own output), whatever the grace, then
/// returns end of input on a quiet or closed pipe, even if a detached
/// descendant still holds the write end. The grace bounds only waiting for
/// more: past [`DRAIN_GRACE`](super::DRAIN_GRACE), a further byte, or a quiet pipe still held
/// open, fails the read. The reader drops the pipe on return, so no thread
/// or descriptor outlives the call. Elsewhere the `Job` closes the pipes
/// when it terminates, and a read blocks until then.
pub(super) struct Gated<R> {
    pub(super) inner: R,
    /// When the command ended; set once.
    #[cfg(unix)]
    finished: Arc<OnceLock<Instant>>,
    /// How long after the end this pipe may still bring more bytes.
    #[cfg(unix)]
    grace: Duration,
    /// Bytes left of those pending when this reader first saw the end.
    #[cfg(unix)]
    pending: Option<usize>,
}

impl<R> Gated<R> {
    #[cfg_attr(not(unix), allow(unused_variables))] // Only Unix reads gate on the end marker.
    pub(super) fn new(inner: R, finished: Arc<OnceLock<Instant>>, grace: Duration) -> Self {
        Self {
            inner,
            #[cfg(unix)]
            finished,
            #[cfg(unix)]
            grace,
            #[cfg(unix)]
            pending: None,
        }
    }
}

#[cfg(unix)]
impl<R: Read + std::os::fd::AsRawFd> Read for Gated<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let fd = self.inner.as_raw_fd();
        loop {
            // Read the end marker before polling: when it was already set,
            // every byte the command wrote is in the pipe, so a quiet poll is final.
            let Some(ended) = self.finished.get().copied() else {
                if poll_readable(fd, POLL)? {
                    return self.inner.read(buf);
                }
                continue;
            };
            let pending = *self.pending.get_or_insert_with(|| pending_bytes(fd));
            if pending > 0 {
                let len = buf.len().min(pending);
                let count = self.inner.read(&mut buf[..len])?;
                self.pending = Some(pending - count);
                return Ok(count);
            }
            let streaming =
                || std::io::Error::other("output kept streaming after the command ended");
            let past = Instant::now() >= ended + self.grace;
            if !poll_readable(fd, Duration::ZERO)? {
                return if past { Err(streaming()) } else { Ok(0) };
            }
            // Readable: more bytes, or the end of a pipe no writer holds.
            let count = self.inner.read(buf)?;
            return if past && count > 0 {
                Err(streaming())
            } else {
                Ok(count)
            };
        }
    }
}

#[cfg(not(unix))]
impl<R: Read> Read for Gated<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

/// Bytes waiting in the pipe `fd` (`FIONREAD`); 0 when it cannot say.
#[cfg(unix)]
fn pending_bytes(fd: std::os::fd::RawFd) -> usize {
    let mut count: libc::c_int = 0;
    // SAFETY: FIONREAD writes one c_int through the live pointer.
    let result = unsafe { libc::ioctl(fd, libc::FIONREAD, &raw mut count) };
    if result < 0 {
        0
    } else {
        usize::try_from(count).unwrap_or(0)
    }
}

#[cfg(unix)]
fn poll_readable(fd: std::os::fd::RawFd, wait: Duration) -> std::io::Result<bool> {
    let mut entry = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let millis = i32::try_from(wait.as_millis()).unwrap_or(i32::MAX);
    // SAFETY: one live pollfd entry; the count is 1.
    let ready = unsafe { libc::poll(&raw mut entry, 1, millis) };
    if ready >= 0 {
        return Ok(ready > 0);
    }
    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::Interrupted {
        Ok(false)
    } else {
        Err(error)
    }
}

/// The pipe gate on Unix, where reads poll the descriptor.
#[cfg(all(test, unix))]
mod tests;
