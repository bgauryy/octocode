//! Blob-only transfers for ghStructure `materialize`: the blobs a listing
//! already named by id, fetched in one request into a throwaway bare
//! repository and read back with `cat-file --batch`.
//!
//! No commit, tree, checkout or shared store is involved: a store holds
//! only the blobs one call asked for and is deleted when the call ends.
//!
//! Never ask git for sizes or listings of this store (`ls-tree -l`,
//! `cat-file -s`, `cat-file --batch-check`): a store that holds only some
//! objects may fetch each missing blob one request at a time to answer
//! them. Sizes and ids come from the REST tree the caller already read.
use super::endpoint::{repository_url, web_root};
use super::process::process_alive;
use super::step::{Step, run, run_with, scoped, step_timeout};
use super::{GitContext, GitError, GitSetup, control};
use octocode_contract_types::ErrorCode;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod frames;
pub(crate) use frames::Frame;
use frames::FrameParser;
#[cfg(test)]
mod tests;

/// Where throwaway stores live under the Octocode home.
const STORE_PARENT: [&str; 2] = ["tmp", "materialize-git"];

/// A bare repository that exists for one materialize call and is removed
/// when the last holder drops it (the call ending, failing or cancelled).
pub(crate) struct BlobStore {
    dir: PathBuf,
    repository_url: String,
}

impl Drop for BlobStore {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The pid a store directory name (`<pid>-<nonce>`) records.
fn owner_pid(name: &str) -> Option<u32> {
    let (pid, nonce) = name.split_once('-')?;
    let hex_nonce = nonce.len() == 32 && nonce.bytes().all(|byte| byte.is_ascii_hexdigit());
    (hex_nonce && !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| pid.parse().ok())
        .flatten()
}

/// Remove the stores of processes that no longer run (a killed call leaves
/// its store behind). A live process's store, this one's included, stays.
fn sweep_stale(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let stale =
            owner_pid(&name).is_some_and(|pid| pid != std::process::id() && !process_alive(pid));
        if stale && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// `oid` per line: what `fetch --stdin` and `cat-file --batch` read.
fn oid_lines(oids: &[String]) -> Vec<u8> {
    let mut lines = Vec::with_capacity(oids.len() * 41);
    for oid in oids {
        lines.extend_from_slice(oid.as_bytes());
        lines.push(b'\n');
    }
    lines
}

impl BlobStore {
    fn create(context: &GitContext<'_>, repository_url: String) -> Result<Self, GitError> {
        let parent = STORE_PARENT
            .iter()
            .fold(context.config.home.clone(), |path, part| path.join(part));
        std::fs::create_dir_all(&parent).map_err(|error| {
            GitError::new(
                ErrorCode::GitFailed,
                format!("Failed to create the blob store directory: {error}"),
            )
        })?;
        sweep_stale(&parent);
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce)
            .map_err(|error| GitError::new(ErrorCode::GitFailed, error.to_string()))?;
        // Owned before `init` runs, so a store the init half-made is removed.
        let store = Self {
            dir: parent.join(format!("{}-{}", std::process::id(), hex::encode(nonce))),
            repository_url,
        };
        run(
            context,
            vec![
                "init".into(),
                "--bare".into(),
                "--quiet".into(),
                "--".into(),
                store.dir.as_os_str().to_owned(),
            ],
            step_timeout(context),
            "initialize blob store",
            None,
        )?;
        Ok(store)
    }

    /// Fetch `oids` (blob ids the remote's tree names) in one transfer. A
    /// remote that refuses an id refuses the whole fetch.
    fn fetch(&self, context: &GitContext<'_>, oids: &[String]) -> Result<(), GitError> {
        let mut args = scoped(
            &self.dir,
            &[
                "-c",
                // Keep the received pack: unpacking it would write one loose
                // file per blob that this store reads once and drops.
                "transfer.unpackLimit=1",
                "-c",
                "gc.auto=0",
                "fetch",
                "--no-tags",
                "--no-write-fetch-head",
                "--quiet",
                "--stdin",
                "--",
            ],
        );
        args.push(self.repository_url.clone().into());
        let input = oid_lines(oids);
        run_with(
            context,
            Step {
                args,
                timeout: step_timeout(context),
                label: "fetch blobs",
                authorization_url: Some(&self.repository_url),
                stdin: Some(&input),
            },
            None,
        )?;
        Ok(())
    }

    /// Read `oids` back, each as one [`Frame`] in the order given. A blob
    /// longer than `limit` is reported by size and its bytes dropped as they
    /// arrive.
    fn read(
        &self,
        context: &GitContext<'_>,
        oids: &[String],
        limit: usize,
        frame: &mut (dyn FnMut(Frame) + Send),
    ) -> Result<(), GitError> {
        let input = oid_lines(oids);
        let mut parser = FrameParser::new(limit);
        run_with(
            context,
            Step {
                args: scoped(&self.dir, &["cat-file", "--batch"]),
                timeout: step_timeout(context),
                label: "read fetched blobs",
                authorization_url: None,
                stdin: Some(&input),
            },
            Some(&mut |chunk| {
                parser
                    .push(chunk, frame)
                    .map_err(|message| GitError::new(ErrorCode::GitFailed, message))
            }),
        )?;
        parser
            .finish()
            .map_err(|message| GitError::new(ErrorCode::GitFailed, message))
    }
}

/// What a materialize call needs to read blobs through git: the clone
/// tool's git, limiter, credential and cancellation, and the web root of
/// the API endpoint.
pub(crate) struct BlobSource {
    setup: Arc<GitSetup>,
    root: url::Url,
}

impl BlobSource {
    /// `None` where git cannot serve the call: no persistent storage, or an
    /// endpoint with no git URL (an API proxy).
    pub(crate) fn new(setup: Arc<GitSetup>) -> Option<Self> {
        if !setup.config.persistent {
            return None;
        }
        let root = web_root(&setup.endpoint).ok()?;
        Some(Self { setup, root })
    }

    /// A throwaway store for `owner/repo`; fails when git is missing or too
    /// old, which the caller answers by reading over REST.
    pub(crate) async fn open(&self, owner: &str, repo: &str) -> Result<BlobSession, GitError> {
        let url = repository_url(&self.root, owner, repo)?;
        let store = self
            .setup
            .off_runtime(move |context| {
                context.git.assert_available(&control(context))?;
                BlobStore::create(context, url)
            })
            .await?;
        Ok(BlobSession {
            setup: Arc::clone(&self.setup),
            store: Arc::new(store),
        })
    }
}

/// One call's store: waves of blobs fetched into it. The directory goes
/// when the session and any wave still running on it are done.
pub(crate) struct BlobSession {
    setup: Arc<GitSetup>,
    store: Arc<BlobStore>,
}

impl BlobSession {
    /// Fetch `oids` in one transfer and read each back. A blob longer than
    /// `limit` is a [`Frame::Oversized`]; one the store lacks, a
    /// [`Frame::Missing`].
    pub(crate) async fn wave(
        &self,
        oids: Vec<String>,
        limit: usize,
    ) -> Result<Vec<Frame>, GitError> {
        let store = Arc::clone(&self.store);
        self.setup
            .off_runtime(move |context| {
                store.fetch(context, &oids)?;
                let mut frames = Vec::with_capacity(oids.len());
                store.read(context, &oids, limit, &mut |frame| frames.push(frame))?;
                Ok(frames)
            })
            .await
    }
}
