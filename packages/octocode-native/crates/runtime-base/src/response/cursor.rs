pub mod issued;

/// The content snapshot of a row plan at `budget` chars per page, of an
/// envelope whose compact rendering has the hex SHA-256 `digest`, cut by the
/// build whose identity is `build` (the runtime's build identity, which every
/// cursor binds to: [`super::page_model::ResponsePageOptions::build_identity`]).
pub fn rows_snapshot(build: &str, budget: usize, digest: &str) -> String {
    format!("response-rows-{build}:{budget}:{digest}")
}

/// One page of one saved response.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Cursor {
    /// The saved response's replay key.
    pub key: [u8; 32],
    /// The digest of the content the pages cut.
    pub digest: [u8; 32],
    /// Rendered characters per response page.
    pub length: usize,
    /// Zero-based response page index.
    pub offset: usize,
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

    /// The content snapshot this cursor's pages were cut from by the build
    /// whose identity is `build`.
    pub fn content_snapshot(&self, build: &str) -> String {
        rows_snapshot(build, self.length, &hex::encode(self.digest))
    }

    /// The page-independent name a saved response is stored under: its
    /// cursor at the first page.
    pub fn resource(&self) -> String {
        format!(
            "{}:{}:{}",
            hex::encode(self.key),
            hex::encode(self.digest),
            self.length
        )
    }
}
