//! The one bounded read of a local source file and the one binary sniff.
//!
//! [`read_source_from`] reads an open source up to a limit, checking between
//! chunks (a cancel stops it). [`read_source`] opens without blocking (a FIFO cannot hang it), checks the
//! opened handle is a regular file, and reads through `take(limit + 1)`, so a
//! file grown past the limit since it was listed is refused without being
//! read whole. [`open_regular`] also refuses a final symlink, for a walk
//! entry that may have been swapped since it was listed. [`sniff`] is the one
//! definition of binary content every tool applies.

use std::borrow::Cow;
use std::io::Read;
use std::path::Path;

/// Why [`read_source`] refused a path.
#[derive(Debug)]
pub enum ReadError {
    /// The length `fstat` or the capped read found, above the limit.
    TooLarge(u64),
    /// A FIFO, a device, a directory, or another non-regular file.
    NotRegular,
    Io(std::io::Error),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge(len) => write!(f, "file is {len} bytes, above the read limit"),
            Self::NotRegular => f.write_str("not a regular file"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

/// Whether bytes are binary or text (see [`sniff`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryVerdict {
    Text,
    Binary,
}

/// Bytes [`sniff`] inspects.
pub const SNIFF_BYTES: usize = 8192;

/// The one binary definition: a NUL byte, or a dense run of C0 control
/// bytes (over 5% of the first [`SNIFF_BYTES`], `\t`/`\n`/`\r` and ANSI CSI
/// sequences aside). UTF-8 validity is an encoding question, not a binary
/// one: Latin-1 and other 8-bit text is text ([`decode`]).
pub fn sniff(bytes: &[u8]) -> BinaryVerdict {
    if has_utf16_bom(bytes) {
        if bytes.len() < SNIFF_BYTES && !bytes.len().is_multiple_of(2) {
            return BinaryVerdict::Binary;
        }
        let mut end = bytes.len().min(SNIFF_BYTES) & !1;
        if end >= SNIFF_BYTES {
            let last = utf16_unit(bytes, end - 2);
            if (0xd800..=0xdbff).contains(&last) {
                end -= 2;
            }
        }
        return decode_unicode(&bytes[..end])
            .map_or(BinaryVerdict::Binary, |text| sniff(text.as_bytes()));
    }
    let sample = &bytes[..bytes.len().min(SNIFF_BYTES)];
    if sample.contains(&0) {
        return BinaryVerdict::Binary;
    }
    let mut counted = 0usize;
    let mut controls = 0usize;
    let mut i = 0;
    while i < sample.len() {
        if sample[i] == 0x1b && sample.get(i + 1) == Some(&0x5b) {
            i += 2;
            while i < sample.len() && !(0x40..=0x7e).contains(&sample[i]) {
                i += 1;
            }
            i += 1;
            continue;
        }
        counted += 1;
        if sample[i] < 0x20 && !matches!(sample[i], b'\t' | b'\n' | b'\r') {
            controls += 1;
        }
        i += 1;
    }
    if counted > 0 && controls * 20 > counted {
        BinaryVerdict::Binary
    } else {
        BinaryVerdict::Text
    }
}

/// Only a declared UTF-16 byte order is recognized; unmarked bytes are not guessed.
pub fn has_utf16_bom(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff])
}

fn utf16_unit(bytes: &[u8], offset: usize) -> u16 {
    let pair = [bytes[offset], bytes[offset + 1]];
    if bytes.starts_with(&[0xff, 0xfe]) {
        u16::from_le_bytes(pair)
    } else {
        u16::from_be_bytes(pair)
    }
}

/// Strict Unicode decoding for consumers that must not infer a legacy encoding.
/// Coordinates refer to decoded text; callers retain the original bytes for digests.
pub fn decode_unicode(bytes: &[u8]) -> Option<Cow<'_, str>> {
    if has_utf16_bom(bytes) {
        if !bytes.len().is_multiple_of(2) {
            return None;
        }
        let units = (2..bytes.len())
            .step_by(2)
            .map(|offset| utf16_unit(bytes, offset));
        char::decode_utf16(units)
            .collect::<Result<String, _>>()
            .ok()
            .map(Cow::Owned)
    } else {
        std::str::from_utf8(&bytes[super::bom_len(bytes)..])
            .ok()
            .map(Cow::Borrowed)
    }
}

/// Whole-source binary validation, including late NULs and malformed declared UTF-16.
pub fn sniff_complete(bytes: &[u8]) -> BinaryVerdict {
    if has_utf16_bom(bytes) {
        return decode_unicode(bytes).map_or(BinaryVerdict::Binary, |text| {
            if text.contains('\0') {
                BinaryVerdict::Binary
            } else {
                sniff(text.as_bytes())
            }
        });
    }
    if bytes.contains(&0) {
        BinaryVerdict::Binary
    } else {
        sniff(bytes)
    }
}

/// Text decoded from file bytes, and the warning when it was not UTF-8.
#[derive(Debug, PartialEq, Eq)]
pub struct Decoded<'a> {
    pub text: Cow<'a, str>,
    pub warning: Option<&'static str>,
}

/// Source text from file bytes: UTF-8 when valid; otherwise the encoding
/// fallback is flagged with a warning. Bytes holding no valid non-ASCII
/// UTF-8 sequence decode as Latin-1 (ISO-8859-1), one char per byte; text
/// that is mostly UTF-8 keeps it and replaces the stray bytes (U+FFFD).
/// `bytes` start a file: a leading UTF-8 BOM is not text of line 1 (as
/// editors and localSearch show it), so no decoding keeps it.
pub fn decode(bytes: &[u8]) -> Decoded<'_> {
    if has_utf16_bom(bytes)
        && let Some(text) = decode_unicode(bytes)
    {
        return Decoded {
            text,
            warning: Some(
                "Decoded BOM-marked UTF-16; byte offsets count the decoded UTF-8 text, while snapshots identify the original file bytes.",
            ),
        };
    }
    Decoded::interior(&bytes[super::bom_len(bytes)..])
}

impl<'a> Decoded<'a> {
    /// [`decode`] of bytes that do not start a file (a window that begins
    /// at a later line): no byte-order mark can lead them, so every U+FEFF
    /// they hold is text.
    pub fn interior(bytes: &'a [u8]) -> Self {
        if let Ok(text) = std::str::from_utf8(bytes) {
            return Self {
                text: Cow::Borrowed(text),
                warning: None,
            };
        }
        if bytes.utf8_chunks().any(|chunk| !chunk.valid().is_ascii()) {
            return Self {
                text: String::from_utf8_lossy(bytes),
                warning: Some("Not valid UTF-8: invalid bytes were replaced with U+FFFD."),
            };
        }
        Self {
            text: Cow::Owned(bytes.iter().map(|&byte| char::from(byte)).collect()),
            warning: Some(
                "Not valid UTF-8: decoded as Latin-1 (ISO-8859-1); characters from other 8-bit encodings may be approximate, and byte offsets count the decoded UTF-8 text.",
            ),
        }
    }
}

/// One file's bytes, read whole within a limit.
#[derive(Debug)]
pub struct Source {
    pub bytes: Vec<u8>,
    /// Metadata of the opened handle, taken after the read: it describes the
    /// bytes read unless the file was written meanwhile (its length then
    /// differs, or its change time is recent).
    pub metadata: std::fs::Metadata,
}

impl Source {
    pub fn verdict(&self) -> BinaryVerdict {
        sniff_complete(&self.bytes)
    }

    pub fn decoded(&self) -> Decoded<'_> {
        decode(&self.bytes)
    }
}

/// Open a regular file without blocking and without following a final
/// symlink; `fstat` the handle to confirm it is one. The path can be swapped
/// (for a symlink, FIFO or directory) between a walk and the open; checking
/// the opened handle closes that window.
pub fn open_regular(path: &Path) -> std::io::Result<(std::fs::File, std::fs::Metadata)> {
    let file = open_nonblocking(path, true)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::other(
            "not a regular file when opened (the path changed after the walk)",
        ));
    }
    Ok((file, metadata))
}

/// Open for reading without blocking; `unfollowed` refuses a final symlink.
fn open_nonblocking(path: &Path, unfollowed: bool) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let nofollow = if unfollowed { libc::O_NOFOLLOW } else { 0 };
        options.custom_flags(nofollow | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    if unfollowed {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT: open a symlink itself, not its target.
        options.custom_flags(0x0020_0000);
    }
    options.open(path)
}

/// The bytes of the regular file at `path`, at most `limit`.
pub fn read_source(path: &Path, limit: u64) -> Result<Source, ReadError> {
    let file = open_nonblocking(path, false).map_err(ReadError::Io)?;
    let metadata = file.metadata().map_err(ReadError::Io)?;
    if !metadata.is_file() {
        return Err(ReadError::NotRegular);
    }
    if metadata.len() > limit {
        return Err(ReadError::TooLarge(metadata.len()));
    }
    let bytes = read_source_from(
        &file,
        Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0)),
        limit,
        || Ok(()),
    )?;
    let metadata = file.metadata().map_err(ReadError::Io)?;
    Ok(Source { bytes, metadata })
}

/// Bytes [`read_source_from`] reads between two checks.
const READ_CHUNK_BYTES: u64 = 64 * 1024;

/// `prefix` (the bytes already read from `reader`) followed by the rest of
/// `reader`, at most `limit` bytes in all: the one bounded read of an open
/// source. It reads through `take(limit + 1)`, so a source grown past the
/// limit fails as [`ReadError::TooLarge`] (with the length read) without
/// being read whole. `check` runs before each chunk; its error (a cancel)
/// ends the read as [`ReadError::Io`].
pub fn read_source_from(
    reader: impl Read,
    prefix: Vec<u8>,
    limit: u64,
    mut check: impl FnMut() -> std::io::Result<()>,
) -> Result<Vec<u8>, ReadError> {
    let mut bytes = prefix;
    let held = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let mut reader = reader.take(limit.saturating_add(1).saturating_sub(held));
    loop {
        check().map_err(ReadError::Io)?;
        let read = (&mut reader)
            .take(READ_CHUNK_BYTES)
            .read_to_end(&mut bytes)
            .map_err(ReadError::Io)?;
        if read == 0 {
            break;
        }
    }
    let read = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if read > limit {
        return Err(ReadError::TooLarge(read));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bom_utf16_decodes_both_orders_without_guessing_or_losing_coordinates() {
        let text = "α😀\r\nnext\n\u{feff}inside";
        for little in [true, false] {
            let mut bytes = if little {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in text.encode_utf16() {
                bytes.extend(if little {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            assert_eq!(sniff_complete(&bytes), BinaryVerdict::Text);
            assert_eq!(decode_unicode(&bytes).as_deref(), Some(text));
            assert_eq!(decode(&bytes).text, text);
            assert!(decode(&bytes).warning.is_some());
        }
        assert_eq!(sniff_complete(b"a\0b\0"), BinaryVerdict::Binary);
        for bytes in [
            vec![0xff, 0xfe, 0x61],
            vec![0xff, 0xfe, 0, 0xd8],
            vec![0xfe, 0xff, 0xdc, 0],
            vec![0xff, 0xfe, 0, 0],
        ] {
            assert_eq!(sniff_complete(&bytes), BinaryVerdict::Binary);
        }
    }

    #[test]
    fn utf16_sniff_boundary_does_not_split_a_surrogate_pair() {
        let mut bytes = vec![0xff, 0xfe];
        for unit in "a"
            .repeat((SNIFF_BYTES - 4) / 2)
            .encode_utf16()
            .chain("😀".encode_utf16())
        {
            bytes.extend(unit.to_le_bytes());
        }
        assert_eq!(sniff(&bytes[..SNIFF_BYTES]), BinaryVerdict::Text);
        assert_eq!(sniff(&bytes), BinaryVerdict::Text);
        assert_eq!(sniff_complete(&bytes), BinaryVerdict::Text);
        bytes.extend([0, 0]);
        assert_eq!(sniff_complete(&bytes), BinaryVerdict::Binary);
    }

    #[test]
    fn a_file_past_the_limit_or_not_regular_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("big.rs");
        std::fs::write(&path, "x".repeat(65)).expect("write");
        assert!(matches!(
            read_source(&path, 64),
            Err(ReadError::TooLarge(65))
        ));
        assert_eq!(read_source(&path, 65).expect("read").bytes.len(), 65);
        assert!(matches!(
            read_source(dir.path(), 1024),
            Err(ReadError::NotRegular)
        ));
        assert!(matches!(
            read_source(&dir.path().join("missing"), 1024),
            Err(ReadError::Io(_))
        ));
    }

    /// One bounded read serves every caller: it continues after the bytes
    /// already read, refuses a source past the limit with the length it
    /// read, and stops at the first failed check between chunks.
    #[test]
    fn a_bounded_read_continues_a_prefix_and_stops_at_its_limit_or_a_check() {
        let source = b"0123456789".repeat(10_000);
        let read = read_source_from(&source[4..], source[..4].to_vec(), 100_000, || Ok(()))
            .expect("within the limit");
        assert_eq!(read, source);
        assert!(matches!(
            read_source_from(&source[..], Vec::new(), 99_999, || Ok(())),
            Err(ReadError::TooLarge(100_000))
        ));
        let mut checks = 0;
        let stopped = read_source_from(&source[..], Vec::new(), 100_000, || {
            checks += 1;
            if checks > 1 {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled",
                ))
            } else {
                Ok(())
            }
        });
        assert!(
            matches!(&stopped, Err(ReadError::Io(error)) if error.kind() == std::io::ErrorKind::Interrupted),
            "{stopped:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_read_follows_a_symlink_and_a_walk_open_refuses_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("target.rs");
        std::fs::write(&target, "fn a() {}").expect("write");
        let link = dir.path().join("link.rs");
        std::os::unix::fs::symlink(&target, &link).expect("link");
        assert_eq!(read_source(&link, 1024).expect("read").bytes, b"fn a() {}");
        assert!(open_regular(&link).is_err());
    }

    #[test]
    fn binary_is_a_nul_or_dense_controls_and_never_an_encoding() {
        assert_eq!(sniff(b"fn a() {}\0"), BinaryVerdict::Binary);
        assert_eq!(
            sniff(b"\x89PNG\r\n\x1a\n\x02\x03\x04"),
            BinaryVerdict::Binary
        );
        assert_eq!(sniff("caf\u{e9}\n".as_bytes()), BinaryVerdict::Text);
        assert_eq!(sniff(b"caf\xe9\n"), BinaryVerdict::Text);
        assert_eq!(sniff(b"\x1b[31mred\x1b[0m\n"), BinaryVerdict::Text);
        assert_eq!(sniff(b""), BinaryVerdict::Text);
        // A NUL past the sniffed bytes does not make text binary.
        let mut late = vec![b'a'; SNIFF_BYTES];
        late.push(0);
        assert_eq!(sniff(&late), BinaryVerdict::Text);
    }

    #[test]
    fn a_char_straddling_the_sniff_boundary_and_stray_bytes_are_text() {
        let mut data = vec![b'a'; SNIFF_BYTES - 2];
        data.extend_from_slice("\u{1f600}".as_bytes());
        data.extend(std::iter::repeat_n(b'b', 256));
        assert_eq!(sniff(&data), BinaryVerdict::Text);
        let mut stray = vec![b'a'; 100];
        stray[50] = 0xff;
        assert_eq!(sniff(&stray), BinaryVerdict::Text);
        let mut controls = vec![b'a'; 100];
        controls[..10].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 14, 15]);
        assert_eq!(sniff(&controls), BinaryVerdict::Binary);
    }

    #[test]
    fn decode_keeps_utf8_and_falls_back_to_latin1_or_replacement() {
        assert_eq!(decode(b"ok").warning, None);
        let latin = decode(b"caf\xe9");
        assert_eq!(latin.text, "caf\u{e9}");
        assert!(latin.warning.is_some_and(|w| w.contains("Latin-1")));
        let mixed = decode(b"\xc3\xa9\xff");
        assert_eq!(mixed.text, "\u{e9}\u{fffd}");
    }

    /// A leading UTF-8 BOM is not text in any decoding: valid UTF-8 stays
    /// borrowed, and the fallbacks neither keep U+FEFF nor show it as
    /// Latin-1 `ï»¿`. A later U+FEFF is text.
    #[test]
    fn decode_drops_a_leading_bom_in_every_decoding() {
        let utf8 = decode(b"\xef\xbb\xbfalpha\n\xef\xbb\xbfbeta");
        assert_eq!(utf8.text, "alpha\n\u{feff}beta");
        assert!(matches!(utf8.text, Cow::Borrowed(_)));
        assert_eq!(decode(b"\xef\xbb\xbf\xc3\xa9\xff").text, "\u{e9}\u{fffd}");
        assert_eq!(decode(b"\xef\xbb\xbfcaf\xe9").text, "caf\u{e9}");
    }

    /// Bytes inside a file keep a leading U+FEFF: only a file's first
    /// bytes can hold its byte-order mark.
    #[test]
    fn interior_bytes_keep_a_leading_feff_as_text() {
        let window = Decoded::interior(b"\xef\xbb\xbfgamma\n");
        assert_eq!(window.text, "\u{feff}gamma\n");
        assert!(window.warning.is_none());
        assert_eq!(
            Decoded::interior(b"\xef\xbb\xbfcaf\xe9").text,
            "\u{feff}caf\u{fffd}"
        );
    }
}
