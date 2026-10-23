//! The whole-response cursor: the one opaque token a response page's
//! `responsePagination.next` carries, as `{"responseSnapshot": token}`. It
//! names the saved response (its replay key and content digest) and the page
//! to serve (scope, page length, offset), so a caller that copies only that
//! field reaches the next page.

use base64::Engine as _;

const PREFIX: &str = "saved-v2:";
/// Key, digest, scope, length and offset.
const BYTES: usize = 32 + 32 + 1 + 8 + 8;
/// Serialized length of every cursor: the prefix and unpadded base64url.
#[cfg(test)]
pub(crate) const CURSOR_CHARS: usize = PREFIX.len() + (BYTES * 4).div_ceil(3);

/// What a response page windows: the `responseScope` input values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PageScope {
    /// Whole rows or row parts of the structured envelope.
    Rows,
    /// A character window of the serialized structured envelope.
    Structured,
}

impl PageScope {
    /// The scope of a page whose `responsePagination.scope` is `scope`.
    pub(crate) fn of_page(scope: &str) -> Option<Self> {
        match scope {
            "rows" => Some(Self::Rows),
            "structuredContent" => Some(Self::Structured),
            _ => None,
        }
    }

    fn byte(self) -> u8 {
        match self {
            Self::Rows => 0,
            Self::Structured => 1,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Rows),
            1 => Some(Self::Structured),
            _ => None,
        }
    }
}

/// The content snapshot of a row plan at `budget` chars per page, of an
/// envelope whose compact rendering has the hex SHA-256 `digest`.
pub(crate) fn rows_snapshot(budget: usize, digest: &str) -> String {
    snapshot_of(PageScope::Rows, budget, digest)
}

/// The content snapshot of a character window over the serialized envelope.
pub(crate) fn window_snapshot(text: &str) -> String {
    snapshot_of(
        PageScope::Structured,
        0,
        &octocode_engine::digest::sha256(text.as_bytes()),
    )
}

/// A content snapshot from its hex `digest`: a row plan's names its page
/// length, a structured window's does not.
fn snapshot_of(scope: PageScope, length: usize, digest: &str) -> String {
    match scope {
        PageScope::Rows => format!("response-rows-v6:{length}:{digest}"),
        PageScope::Structured => format!("response-v1:{digest}"),
    }
}

/// One page of one saved response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Cursor {
    /// The saved response's replay key.
    pub(crate) key: [u8; 32],
    /// The digest of the content the pages cut.
    pub(crate) digest: [u8; 32],
    pub(crate) scope: PageScope,
    /// Chars per page (`responseLength`).
    pub(crate) length: usize,
    /// The page's offset: a zero-based page index for rows, else a UTF-16
    /// offset into the serialized envelope.
    pub(crate) offset: usize,
}

impl Cursor {
    /// The cursor of the page at `offset` of the content whose snapshot is
    /// `snapshot` ([`rows_snapshot`] or [`window_snapshot`]).
    pub(crate) fn of_snapshot(
        key: [u8; 32],
        snapshot: &str,
        scope: PageScope,
        length: usize,
        offset: usize,
    ) -> Option<Self> {
        let (_, digest) = snapshot.rsplit_once(':')?;
        let digest = hex::decode(digest).ok()?.try_into().ok()?;
        Some(Self {
            key,
            digest,
            scope,
            length,
            offset,
        })
    }

    pub(crate) fn encode(&self) -> String {
        let mut bytes = Vec::with_capacity(BYTES);
        bytes.extend_from_slice(&self.key);
        bytes.extend_from_slice(&self.digest);
        bytes.push(self.scope.byte());
        bytes.extend_from_slice(&(self.length as u64).to_be_bytes());
        bytes.extend_from_slice(&(self.offset as u64).to_be_bytes());
        let base64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        format!("{PREFIX}{}", base64.encode(bytes))
    }

    pub(crate) fn decode(token: &str) -> Option<Self> {
        let base64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let bytes = base64.decode(token.strip_prefix(PREFIX)?).ok()?;
        if bytes.len() != BYTES {
            return None;
        }
        let (key, rest) = bytes.split_at(32);
        let (digest, rest) = rest.split_at(32);
        let (scope, rest) = rest.split_at(1);
        let (length, offset) = rest.split_at(8);
        let number = |bytes: &[u8]| -> Option<usize> {
            usize::try_from(u64::from_be_bytes(bytes.try_into().ok()?)).ok()
        };
        Some(Self {
            key: key.try_into().ok()?,
            digest: digest.try_into().ok()?,
            scope: PageScope::from_byte(scope[0])?,
            length: number(length)?.max(1),
            offset: number(offset)?,
        })
    }

    /// The content snapshot this cursor's pages were cut from.
    pub(crate) fn content_snapshot(&self) -> String {
        snapshot_of(self.scope, self.length, &hex::encode(self.digest))
    }

    /// The page-independent name a saved response is stored under: its
    /// cursor at the first page.
    pub(crate) fn resource(&self) -> String {
        Self {
            offset: 0,
            ..self.clone()
        }
        .encode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(scope: PageScope, offset: usize) -> Cursor {
        Cursor {
            key: [7; 32],
            digest: [9; 32],
            scope,
            length: 600,
            offset,
        }
    }

    #[test]
    fn a_cursor_round_trips_at_a_fixed_length() {
        for scope in [PageScope::Rows, PageScope::Structured] {
            for offset in [0, 1, 4_096, usize::MAX >> 1] {
                let token = cursor(scope, offset).encode();
                assert_eq!(token.len(), CURSOR_CHARS, "{token}");
                assert_eq!(Cursor::decode(&token), Some(cursor(scope, offset)));
            }
        }
        assert_eq!(CURSOR_CHARS, 117);
    }

    #[test]
    fn content_snapshots_match_the_pagers_formats() {
        let full = "{\"results\":[]}";
        let rows = rows_snapshot(600, &octocode_engine::digest::sha256(full.as_bytes()));
        let parsed = Cursor::of_snapshot([1; 32], &rows, PageScope::Rows, 600, 2).expect("rows");
        assert_eq!(parsed.content_snapshot(), rows);
        let window = window_snapshot(full);
        let parsed =
            Cursor::of_snapshot([1; 32], &window, PageScope::Structured, 600, 2).expect("window");
        assert_eq!(parsed.content_snapshot(), window);
    }

    #[test]
    fn malformed_tokens_do_not_decode() {
        let token = cursor(PageScope::Rows, 3).encode();
        for bad in [
            "",
            "saved-v1:abc",
            &token[..token.len() - 1],
            &format!("{token}A"),
            &token.replace(PREFIX, "saved-v3:"),
        ] {
            assert_eq!(Cursor::decode(bad), None, "{bad}");
        }
        // An unknown scope byte.
        let base64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let mut bytes = base64.decode(&token[PREFIX.len()..]).expect("bytes");
        bytes[64] = 0xff;
        let scoped = format!("{PREFIX}{}", base64.encode(bytes));
        assert_eq!(Cursor::decode(&scoped), None);
    }
}
