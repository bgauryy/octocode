//! Git LFS pointers: a blob whose whole text is a pointer stands in for an object this
//! tool cannot read, and the row says so.

const VERSION_LINE: &str = "version https://git-lfs.github.com/spec/v1";
/// The spec bounds a pointer file below this many bytes.
const MAX_POINTER_BYTES: usize = 1024;

/// An LFS pointer's object, as the pointer states it.
pub(super) struct Pointer {
    pub(super) size: u64,
}

/// The pointer a blob is, by the spec's format: `version` first, then `key value` lines
/// in key order, with a `sha256` `oid` and a `size`. Text that only mentions the spec is not one.
pub(super) fn parse(bytes: &[u8]) -> Option<Pointer> {
    if bytes.len() > MAX_POINTER_BYTES {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.lines();
    if lines.next()? != VERSION_LINE {
        return None;
    }
    let (mut oid, mut size) = (false, None);
    for line in lines {
        let (key, value) = line.split_once(' ')?;
        if key.is_empty()
            || value.is_empty()
            || !key
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        {
            return None;
        }
        match key {
            "oid" => {
                let hex = value.strip_prefix("sha256:")?;
                oid = hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit());
                if !oid {
                    return None;
                }
            }
            "size" => size = Some(value.parse::<u64>().ok()?),
            _ => {}
        }
    }
    oid.then_some(())?;
    Some(Pointer { size: size? })
}

/// The row's warning for a pointer: what it is, the object's size and how to read it.
#[expect(
    clippy::expect_used,
    reason = "static https URL, valid by construction"
)]
pub(super) fn warning(pointer: &Pointer, owner: &str, repo: &str, sha: &str, path: &str) -> String {
    let mut media =
        url::Url::parse("https://media.githubusercontent.com/media").expect("fixed base");
    media
        .path_segments_mut()
        .expect("an https base has segments")
        .extend(
            [owner, repo]
                .into_iter()
                .chain(sha.split('/'))
                .chain(path.split('/')),
        );
    format!(
        "This file is a Git LFS pointer, not the file: its object is {} bytes and ghGetFileContent cannot read LFS objects. \
         Download it from {media}",
        pointer.size
    )
}
