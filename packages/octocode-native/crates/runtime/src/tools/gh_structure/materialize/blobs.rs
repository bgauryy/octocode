//! Where a materialize call's file bytes come from besides one REST read per
//! file: blobs fetched through git by the ids the listing named, a wave at a
//! time. A wave is the files whose listed sizes add up to what the call may
//! read (at least one file), fetched in one transfer.
//!
//! Git is a faster way to the same bytes, never another outcome: each blob
//! reaches the writer as the read the REST path would have produced, and
//! anything git does not serve (git missing, a server that refuses the ids,
//! a blob it lacks) is read over REST, silently.
use super::*;
use crate::tools::gh_git::blobs::{BlobSession, BlobSource, Frame};
use octocode_contract_types::ErrorCode;
use std::collections::HashMap;

/// What a wave brought back for one blob id.
pub(super) enum Fetched {
    Bytes(bytes::Bytes),
    /// Longer than the call may read.
    Oversized,
}

/// The git side of one materialize call.
pub(super) struct Blobs<'a> {
    /// Cleared once git cannot serve this call: the rest is read over REST.
    source: Option<&'a BlobSource>,
    session: Option<BlobSession>,
}

impl<'a> Blobs<'a> {
    pub(super) fn new(source: Option<&'a BlobSource>) -> Self {
        Self {
            source,
            session: None,
        }
    }

    /// Fetch `oids` in one wave and return what came back; an id missing from
    /// the map is read over REST. Fails only when the call itself is over
    /// (cancelled or out of time), as a REST read would.
    pub(super) async fn wave(
        &mut self,
        owner: &str,
        repo: &str,
        oids: Vec<String>,
        limit: usize,
    ) -> Result<HashMap<String, Fetched>, ProviderError> {
        let Some(source) = self.source.filter(|_| !oids.is_empty()) else {
            return Ok(HashMap::new());
        };
        if self.session.is_none() {
            match source.open(owner, repo).await {
                Ok(session) => self.session = Some(session),
                Err(error) => return self.refused(&error),
            }
        }
        let Some(session) = &self.session else {
            return Ok(HashMap::new());
        };
        match session.wave(oids, limit).await {
            Ok(frames) => Ok(frames
                .into_iter()
                .filter_map(|frame| match frame {
                    Frame::Blob { oid, bytes } => Some((oid, Fetched::Bytes(bytes.into()))),
                    Frame::Oversized { oid, .. } => Some((oid, Fetched::Oversized)),
                    Frame::Missing { .. } => None,
                })
                .collect()),
            Err(error) => self.refused(&error),
        }
    }

    /// Git did not serve the wave. The call ending is the call's failure; any
    /// other reason (git missing, a refused id, a failed transfer) leaves git
    /// off for the rest of the call, which reads over REST.
    fn refused(
        &mut self,
        error: &crate::tools::gh_git::GitError,
    ) -> Result<HashMap<String, Fetched>, ProviderError> {
        let kind = match error.code {
            ErrorCode::Timeout => ProviderErrorKind::Timeout,
            ErrorCode::Cancelled => ProviderErrorKind::Cancelled,
            _ => {
                self.source = None;
                self.session = None;
                return Ok(HashMap::new());
            }
        };
        Err(ProviderError::new(kind, error.message.clone()))
    }
}

/// How many of `entries` make the next wave: files that read over the
/// network (not credential-named, not past `read_limit`) until their listed
/// sizes reach `read_limit`, at least one entry. Entries that need no read
/// (folders, withheld, oversized) ride along.
pub(super) fn wave_len(entries: &[TreeEntry], read_limit: usize) -> usize {
    let mut listed = 0_u64;
    for (index, entry) in entries.iter().enumerate() {
        let size = entry.size.unwrap_or(0);
        let reads = entry.kind == EntryKind::File
            && size <= read_limit as u64
            && !crate::policy::discovery::is_sensitive_repository_file(&entry.path);
        if reads {
            listed += size;
        }
        if listed >= read_limit as u64 {
            return index + 1;
        }
    }
    entries.len()
}
