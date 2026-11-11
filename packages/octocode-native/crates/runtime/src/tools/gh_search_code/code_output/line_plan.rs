//! The file reads a page's line hits need, planned for every row and sent
//! for the rows a page shows: a response window that holds a few rows reads
//! a few files ([`super::super::part`]).
use crate::security::scan::ContentScan;
use crate::tools::gh_search_code::lines::{FileHits, Listing, Window, resolve_files};
use octocode_contract_types::GhSearchCodeQuery;
use octocode_github::{GitHubProvider, ProviderError, RequestContext};
use std::ops::Range;

/// Line hits for the files of a `match:"file"` page: a ref search is read
/// at the requested ref's commit; a default-branch search (repo-scoped or
/// owner-wide) at each hit's indexed commit (from its `html_url`), the
/// commit the index matched.
pub(in crate::tools::gh_search_code) struct Resolution {
    /// Common commit for page-level verification; file rows retain their own.
    pub(in crate::tools::gh_search_code) sha: Option<String>,
    pub(in crate::tools::gh_search_code) reference: Option<String>,
    /// Per row: the commit it was read at, and its hits.
    pub(in crate::tools::gh_search_code) hits: Vec<(String, FileHits)>,
}

/// `(owner, repo, commit)` a group of rows is read at.
pub(super) type RepoCommit = (String, String, String);

/// One row's read: its index on the page, its path, and the key of the
/// blob it holds (`<blob sha>.<extension>`) when the index named one.
pub(super) type Read = (usize, String, Option<String>);

/// The rows read at one repository and commit.
struct Group {
    key: RepoCommit,
    /// The distinct files read: identical blobs are read once.
    paths: Vec<String>,
    /// Each file's hits once read.
    files: Vec<Option<FileHits>>,
}

/// Every row's read, planned before any is sent.
pub(in crate::tools::gh_search_code) struct LinePlan {
    /// A ref search's ref and the commit it resolved to.
    reference: Option<(String, String)>,
    groups: Vec<Group>,
    /// Per page row: its group and file, `None` for a row no group reads
    /// (it keeps its fragments).
    rows: Vec<Option<(usize, usize)>>,
    keywords: Vec<String>,
    window: Window,
}

impl LinePlan {
    pub(super) fn new(
        query: &GhSearchCodeQuery,
        rows: usize,
        groups: Vec<(RepoCommit, Vec<Read>)>,
        reference: Option<(String, String)>,
    ) -> Self {
        let mut at = vec![None; rows];
        let groups = groups
            .into_iter()
            .enumerate()
            .map(|(group, (key, reads))| {
                let mut paths: Vec<String> = Vec::new();
                let mut blobs: Vec<Option<String>> = Vec::new();
                for (index, path, blob) in reads {
                    let file = blob
                        .as_deref()
                        .and_then(|blob| {
                            blobs.iter().position(|seen| seen.as_deref() == Some(blob))
                        })
                        .unwrap_or_else(|| {
                            paths.push(path);
                            blobs.push(blob);
                            paths.len() - 1
                        });
                    if let Some(slot) = at.get_mut(index) {
                        *slot = Some((group, file));
                    }
                }
                Group {
                    key,
                    files: vec![None; paths.len()],
                    paths,
                }
            })
            .collect();
        let window = query
            .context_lines
            .as_ref()
            .map_or_else(Default::default, |lines| Window {
                before: usize::try_from(lines.before).unwrap_or(0),
                after: usize::try_from(lines.after).unwrap_or(0),
            });
        Self {
            reference,
            groups,
            rows: at,
            keywords: query.keywords.iter().map(ToString::to_string).collect(),
            window,
        }
    }

    /// Read the files of `rows` not read yet, concurrently across groups.
    pub(in crate::tools::gh_search_code) async fn read(
        &mut self,
        provider: &GitHubProvider,
        rows: Range<usize>,
        context: &RequestContext,
        security: &(impl ContentScan + Sync),
    ) -> Result<(), ProviderError> {
        let mut wanted: Vec<Vec<usize>> = vec![Vec::new(); self.groups.len()];
        for &(group, file) in self.rows.get(rows).into_iter().flatten().flatten() {
            if self.groups[group].files[file].is_none() && !wanted[group].contains(&file) {
                wanted[group].push(file);
            }
        }
        let listing = Listing {
            keywords: &self.keywords,
            goal: "",
            window: self.window,
        };
        let batches = self
            .groups
            .iter()
            .zip(&wanted)
            .filter(|(_, files)| !files.is_empty())
            .map(|(group, files)| {
                let paths = files
                    .iter()
                    .map(|&file| group.paths[file].clone())
                    .collect::<Vec<_>>();
                let (owner, repo, sha) = &group.key;
                let listing = &listing;
                async move {
                    resolve_files(
                        provider, owner, repo, sha, &paths, listing, context, security,
                    )
                    .await
                }
            })
            .collect::<Vec<_>>();
        let resolved = futures_util::future::join_all(batches).await;
        let groups = wanted
            .iter()
            .enumerate()
            .filter(|(_, files)| !files.is_empty());
        for ((group, files), result) in groups.zip(resolved) {
            for (&file, hits) in files.iter().zip(result?) {
                self.groups[group].files[file] = Some(hits);
            }
        }
        Ok(())
    }

    /// The hits of `rows` (read by [`LinePlan::read`]): a row no group
    /// reads keeps its fragments (`Unavailable` at no commit).
    pub(in crate::tools::gh_search_code) fn resolution(&self, rows: Range<usize>) -> Resolution {
        let hits = self
            .rows
            .get(rows)
            .unwrap_or_default()
            .iter()
            .map(|slot| match slot {
                Some((group, file)) => {
                    let group = &self.groups[*group];
                    let hits = group.files[*file].clone().unwrap_or(FileHits::Unavailable);
                    (group.key.2.clone(), hits)
                }
                None => (String::new(), FileHits::Unavailable),
            })
            .collect::<Vec<_>>();
        let sha = match &self.reference {
            Some((_, sha)) => Some(sha.clone()),
            None => {
                let mut shas = hits
                    .iter()
                    .map(|(sha, _)| sha.as_str())
                    .filter(|sha| !sha.is_empty());
                let first = shas.next().map(str::to_owned);
                first.filter(|sha| shas.all(|other| other == sha))
            }
        };
        Resolution {
            sha,
            reference: self.reference.as_ref().map(|(name, _)| name.clone()),
            hits,
        }
    }
}
