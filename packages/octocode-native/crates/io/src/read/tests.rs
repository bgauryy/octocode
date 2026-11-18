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
