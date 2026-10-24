//! The one bounded read of a stream, sync ([`read_limited`]) and async
//! ([`read_limited_async`], a child's pipe): at most `limit` bytes, and an
//! input that holds more is reported, never silently cut to a prefix.
//! (The HTTP body reader is [`retry::read_bounded`](crate::retry::read_bounded);
//! the engine, a leaf below this crate, keeps its own cancellable source
//! read.)
use std::io::{self, Read};
use tokio::io::{AsyncRead, AsyncReadExt};

/// Read at most `limit` bytes from `reader`; `Ok(None)` when it holds more.
/// The read stops one byte past `limit`, so an endless reader cannot grow
/// the buffer beyond it.
pub fn read_limited(reader: impl Read, limit: u64) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok(within(bytes, limit))
}

/// [`read_limited`] for an async reader.
pub async fn read_limited_async(
    reader: impl AsyncRead + Unpin,
    limit: u64,
) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .await?;
    Ok(within(bytes, limit))
}

fn within(bytes: Vec<u8>, limit: u64) -> Option<Vec<u8>> {
    (bytes.len() as u64 <= limit).then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::{read_limited, read_limited_async};

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

    #[tokio::test]
    async fn the_async_read_has_the_same_bound() {
        assert_eq!(
            read_limited_async(&b"abcd"[..], 4).await.unwrap(),
            Some(b"abcd".to_vec())
        );
        assert_eq!(read_limited_async(&b"abcde"[..], 4).await.unwrap(), None);
        assert_eq!(
            read_limited_async(tokio::io::repeat(7), 1024)
                .await
                .unwrap(),
            None
        );
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
