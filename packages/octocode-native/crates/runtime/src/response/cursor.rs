/// The content snapshot of a row plan at `budget` chars per page, of an
/// envelope whose compact rendering has the hex SHA-256 `digest`.
pub(crate) fn rows_snapshot(budget: usize, digest: &str) -> String {
    format!("response-rows-v7:{budget}:{digest}")
}

/// One page of one saved response.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Cursor {
    /// The saved response's replay key.
    pub(crate) key: [u8; 32],
    /// The digest of the content the pages cut.
    pub(crate) digest: [u8; 32],
    /// Rendered characters per response page.
    pub(crate) length: usize,
    /// Zero-based response page index.
    pub(crate) offset: usize,
}

impl Cursor {
    /// The cursor of the page at `offset` of the content whose snapshot is
    /// `snapshot` ([`rows_snapshot`]).
    pub(crate) fn of_snapshot(
        key: [u8; 32],
        snapshot: &str,
        length: usize,
        offset: usize,
    ) -> Option<Self> {
        let (_, digest) = snapshot.rsplit_once(':')?;
        let digest = hex::decode(digest).ok()?.try_into().ok()?;
        Some(Self {
            key,
            digest,
            length,
            offset,
        })
    }

    /// The content snapshot this cursor's pages were cut from.
    pub(crate) fn content_snapshot(&self) -> String {
        rows_snapshot(self.length, &hex::encode(self.digest))
    }

    /// The page-independent name a saved response is stored under: its
    /// cursor at the first page.
    pub(crate) fn resource(&self) -> String {
        format!(
            "{}:{}:{}",
            hex::encode(self.key),
            hex::encode(self.digest),
            self.length
        )
    }
}
