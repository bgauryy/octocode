//! What a scan window pins of the candidates before its cut. A resume skips them
//! by count, which reads as the same place only over the same files: the digest
//! of each one's path and stamp, in walk order, says whether it is.
use std::path::Path;

use super::HashWriter;
use crate::stamp::SourceStamp;

/// The digest of the candidates a scan has passed, in walk order.
#[derive(Clone, Default)]
pub struct ListedFiles(HashWriter);

impl ListedFiles {
    /// `path` comes next, as `stamp` reads it (`None`: no stamp to read, which
    /// is itself what a later read of a removed file sees).
    pub fn add(&mut self, path: &Path, stamp: Option<SourceStamp>) {
        self.0.update(path.as_os_str().as_encoded_bytes());
        self.0.update([0, u8::from(stamp.is_some())]);
        if let Some(stamp) = stamp {
            for field in [stamp.size, stamp.modified_ns, stamp.changed_ns, stamp.inode] {
                self.0.update(field.to_le_bytes());
            }
        }
    }

    /// The digest of what was added so far.
    #[must_use]
    pub fn hex(&self) -> String {
        hex::encode(self.0.clone().finalize())
    }
}

/// The stamp an `lstat` of `path` reads now.
pub(crate) fn stamp_of(path: &Path) -> Option<SourceStamp> {
    SourceStamp::of(&std::fs::symlink_metadata(path).ok()?)
}
