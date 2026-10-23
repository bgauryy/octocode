//! The one bounded synchronous read: at most `limit` bytes of a reader, and
//! an input that holds more is reported, never silently cut to a prefix.
//! (The async body reader is [`retry::read_bounded`](crate::retry::read_bounded);
//! the engine, a leaf below this crate, keeps its own cancellable source
//! read.)
use std::io::{self, Read};

/// Read at most `limit` bytes from `reader`; `Ok(None)` when it holds more.
/// The read stops one byte past `limit`, so an endless reader cannot grow
/// the buffer beyond it.
pub fn read_limited(reader: impl Read, limit: u64) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= limit).then_some(bytes))
}

#[cfg(test)]
mod tests {
    use super::read_limited;

    #[test]
    fn input_at_the_limit_is_read_and_one_byte_more_is_reported() {
        assert_eq!(
            read_limited(&b"abcd"[..], 4).unwrap(),
            Some(b"abcd".to_vec())
        );
        assert_eq!(read_limited(&b"abcde"[..], 4).unwrap(), None);
        assert_eq!(read_limited(&b""[..], 0).unwrap(), Some(Vec::new()));
        assert_eq!(read_limited(&b"a"[..], 0).unwrap(), None);
    }

    #[test]
    fn an_endless_reader_stops_past_the_limit() {
        assert_eq!(read_limited(std::io::repeat(7), 1024).unwrap(), None);
    }

    #[test]
    fn a_failing_reader_is_an_io_error() {
        struct Broken;
        impl std::io::Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("broken"))
            }
        }
        assert!(read_limited(Broken, 8).is_err());
    }
}
