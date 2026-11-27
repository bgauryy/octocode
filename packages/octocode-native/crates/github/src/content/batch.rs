//! Many files of one commit read in a few requests: one GraphQL document names
//! each file's blob and each of their folders' trees by `<commit>:<path>`, so
//! a page of N hit files in K folders costs one request instead of N + K REST
//! reads. A file the answer vouches for is kept as the pinned answer of its
//! own contents read, beside the proof that no symlink is on its path: its
//! later read at that commit (the caller's own, or another tool's) sends
//! nothing. A file it cannot vouch for is left to that read, which asks REST
//! as before: GraphQL only ever saves requests, it never decides an answer.
//!
//! The answer vouches for a file when every one of these holds:
//! - its folder's object is a git tree: GitHub resolves `<commit>:<dir>`
//!   through real directories only, so no directory above the file is a link;
//! - that tree lists the file as a regular blob (mode `100644` or `100755`),
//!   not a link, submodule or directory;
//! - its blob is that entry's object, text, whole (not `isBinary`, not
//!   `isTruncated`, `byteSize` equal to the text's length);
//! - the text hashes to the blob's git object id (`sha1("blob <n>\0" + text)`),
//!   so it is byte for byte what the REST contents read would answer.
use reqwest::header::{CONTENT_TYPE, ETAG, HeaderMap, HeaderValue};
use serde::Deserialize;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;

use super::links::split_parent;
use super::{ContentRequest, GitHubProvider, is_full_sha};
use crate::{RequestContext, ResponsePage};

/// Files one GraphQL request names (their folders ride along): a page's
/// worth, so each answer stays a few megabytes at most (GitHub's blob text
/// is at most 512,000 bytes; a larger one comes back `isTruncated` and its
/// read asks REST).
const FILES_PER_REQUEST: usize = 10;

/// The git modes of a regular file: plain and executable.
const REGULAR_MODES: [u32; 2] = [0o100_644, 0o100_755];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Blob {
    oid: String,
    byte_size: usize,
    #[serde(default)]
    is_binary: bool,
    #[serde(default)]
    is_truncated: bool,
    text: Option<String>,
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    mode: u32,
    #[serde(rename = "type")]
    kind: String,
    oid: String,
}

#[derive(Deserialize)]
struct Tree {
    entries: Vec<Entry>,
}

/// A folder object: the tree itself, or for the root, the commit whose tree
/// it is.
#[derive(Deserialize)]
struct Folder {
    entries: Option<Vec<Entry>>,
    tree: Option<Tree>,
}

impl Folder {
    fn entries(self) -> Option<Vec<Entry>> {
        self.entries.or(self.tree.map(|tree| tree.entries))
    }
}

/// The selection that reads a folder's entries: a directory's tree, or the
/// root's through its commit.
const FOLDER_FIELDS: &str = "... on Tree { entries { name mode type oid } } ... on Commit { tree { entries { name mode type oid } } }";
const BLOB_FIELDS: &str = "... on Blob { oid byteSize isBinary isTruncated text }";

/// Files of one repository at one commit, which a caller reads together
/// ([`GitHubProvider::warm_files_at_commits`]).
pub struct FilesAtCommit<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    /// A full commit SHA.
    pub commit: &'a str,
    pub paths: &'a [String],
}

/// One file a request names: its group and path.
struct Wanted<'a> {
    group: usize,
    path: &'a str,
}

/// One request's document and variables for `files`: each group is the
/// alias `r{g}` (`repository(owner:$o{g},name:$n{g})`), each file `f{i}` and
/// each distinct folder `d{j}` under it. Returns the folders, by group and
/// directory, in alias order.
fn document<'a>(
    groups: &[FilesAtCommit<'_>],
    files: &[Wanted<'a>],
) -> (String, Value, Vec<(usize, &'a str)>) {
    let mut folders: Vec<(usize, &str)> = Vec::new();
    for file in files {
        let folder = (file.group, split_parent(file.path).0);
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    let mut variables = json!({});
    let mut declared = Vec::new();
    let mut repositories: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (at, file) in files.iter().enumerate() {
        declared.push(format!("$f{at}:String!"));
        variables[format!("f{at}")] = json!(format!("{}:{}", groups[file.group].commit, file.path));
        repositories.entry(file.group).or_default().push(format!(
            "f{at}: object(expression:$f{at}) {{ {BLOB_FIELDS} }}"
        ));
    }
    for (at, (group, dir)) in folders.iter().enumerate() {
        declared.push(format!("$d{at}:String!"));
        let commit = groups[*group].commit;
        let expression = if dir.is_empty() {
            commit.to_owned()
        } else {
            format!("{commit}:{dir}")
        };
        variables[format!("d{at}")] = json!(expression);
        repositories.entry(*group).or_default().push(format!(
            "d{at}: object(expression:$d{at}) {{ {FOLDER_FIELDS} }}"
        ));
    }
    let mut selections = Vec::new();
    for (group, fields) in &repositories {
        declared.push(format!("$o{group}:String!,$n{group}:String!"));
        variables[format!("o{group}")] = json!(groups[*group].owner);
        variables[format!("n{group}")] = json!(groups[*group].repo);
        selections.push(format!(
            "r{group}: repository(owner:$o{group},name:$n{group}){{ {} }}",
            fields.join(" ")
        ));
    }
    (
        format!(
            "query({}){{ {} }}",
            declared.join(","),
            selections.join(" ")
        ),
        variables,
        folders,
    )
}

/// The git object id of a blob holding `bytes`.
fn blob_oid(bytes: &[u8]) -> String {
    let mut digest = Sha1::new();
    digest.update(format!("blob {}\0", bytes.len()).as_bytes());
    digest.update(bytes);
    hex::encode(digest.finalize())
}

/// `text` when the answer vouches for it as the regular file `name` of
/// `entries` (module doc), else `None`.
fn vouched(blob: Blob, entries: &[Entry], name: &str) -> Option<(String, String)> {
    let entry = entries.iter().find(|entry| entry.name == name)?;
    if entry.kind != "blob" || !REGULAR_MODES.contains(&entry.mode) {
        return None;
    }
    let text = blob.text?;
    let whole = !blob.is_binary && !blob.is_truncated && blob.byte_size == text.len();
    (whole
        && entry.oid.eq_ignore_ascii_case(&blob.oid)
        && blob_oid(text.as_bytes()).eq_ignore_ascii_case(&blob.oid))
    .then(|| (text, blob.oid.to_ascii_lowercase()))
}

/// The page a REST contents read of the raw media type answers for a file
/// holding `text`: its bytes, typed raw, with the blob id as its `ETag`.
fn raw_page(text: String, oid: &str) -> Option<ResponsePage> {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.github.raw; charset=utf-8"),
    );
    headers.insert(ETAG, HeaderValue::from_str(&format!("\"{oid}\"")).ok()?);
    Some(ResponsePage {
        status: 200,
        headers,
        body: text.into(),
        next: None,
        redirected: false,
    })
}

impl GitHubProvider {
    /// Reads the files of `groups` (each its repository's paths at a full
    /// commit SHA) by GraphQL, [`FILES_PER_REQUEST`] to a request whatever
    /// their repositories, and keeps each file the answer vouches for as the
    /// pinned answer of its contents read with its link proof (module doc).
    /// Nothing is returned and nothing fails: a path this does not keep
    /// (already answered, not vouched for, or any request error) is read as
    /// before by whoever reads it. A cancelled or expired request stops here;
    /// the reads then report it.
    ///
    /// It asks only when its answers can be kept, a credential is resolved
    /// and GraphQL is not cooling down, and only for paths whose contents
    /// read would otherwise be sent.
    pub async fn warm_files_at_commits(
        &self,
        groups: &[FilesAtCommit<'_>],
        context: &RequestContext,
    ) {
        if !self.transport.keeps_answers()
            || context.resolved_credential().is_none()
            || !self.transport.graphql_available(context)
        {
            return;
        }
        let groups = groups
            .iter()
            .filter(|group| is_full_sha(group.commit))
            .map(|group| (group, group.commit.to_ascii_lowercase()))
            .collect::<Vec<_>>();
        let groups = groups
            .iter()
            .map(|(group, commit)| FilesAtCommit {
                owner: group.owner,
                repo: group.repo,
                commit: commit.as_str(),
                paths: group.paths,
            })
            .collect::<Vec<_>>();
        let mut wanted: Vec<Wanted<'_>> = Vec::new();
        for (at, group) in groups.iter().enumerate() {
            for path in group.paths {
                let answered = self
                    .contents_spec(&group.request(path), group.commit)
                    .is_ok_and(|spec| self.transport.answers_unasked(&spec, context));
                let repeated = wanted
                    .iter()
                    .any(|known| known.group == at && known.path == path.as_str());
                if !path.is_empty() && !answered && !repeated {
                    wanted.push(Wanted { group: at, path });
                }
            }
        }
        for files in wanted.chunks(FILES_PER_REQUEST) {
            if context.check().is_err() {
                return;
            }
            self.warm_chunk(&groups, files, context).await;
        }
    }

    async fn warm_chunk(
        &self,
        groups: &[FilesAtCommit<'_>],
        files: &[Wanted<'_>],
        context: &RequestContext,
    ) {
        let (document, variables, folders) = document(groups, files);
        let Ok(mut page) = self
            .transport
            .execute_graphql(&document, variables, context)
            .await
        else {
            return;
        };
        let Some(Value::Object(mut repositories)) = page.data.take() else {
            return;
        };
        // An error names the repository alias it nulled (`["r0"]`) or the
        // object under it (`["r0", "f3"]`); an error anywhere else leaves
        // nothing of the answer trusted.
        let mut failed = std::collections::BTreeSet::new();
        for error in &page.errors {
            let names = error
                .path
                .iter()
                .take(2)
                .map(|name| name.as_str())
                .collect::<Option<Vec<_>>>();
            match names.as_deref() {
                Some([repository]) if repository.starts_with('r') => {
                    failed.insert((*repository).to_owned());
                }
                Some([repository, object]) if repository.starts_with('r') => {
                    failed.insert(format!("{repository}/{object}"));
                }
                _ => return,
            }
        }
        let mut object = |group: usize, alias: &str| -> Option<Value> {
            let repository = format!("r{group}");
            if failed.contains(&repository) || failed.contains(&format!("{repository}/{alias}")) {
                return None;
            }
            repositories
                .get_mut(&repository)
                .and_then(|objects| objects.get_mut(alias))
                .map(Value::take)
        };
        let mut trees: BTreeMap<(usize, &str), Vec<Entry>> = BTreeMap::new();
        for (at, (group, dir)) in folders.iter().enumerate() {
            if let Some(entries) = object(*group, &format!("d{at}"))
                .and_then(|folder| serde_json::from_value::<Folder>(folder).ok())
                .and_then(Folder::entries)
            {
                trees.insert((*group, dir), entries);
            }
        }
        for (at, file) in files.iter().enumerate() {
            let (dir, name) = split_parent(file.path);
            let Some(entries) = trees.get(&(file.group, dir)) else {
                continue;
            };
            let Some((text, oid)) = object(file.group, &format!("f{at}"))
                .and_then(|blob| serde_json::from_value::<Blob>(blob).ok())
                .and_then(|blob| vouched(blob, entries, name))
            else {
                continue;
            };
            let group = &groups[file.group];
            let (Ok(spec), Some(answer)) = (
                self.contents_spec(&group.request(file.path), group.commit),
                raw_page(text, &oid),
            ) else {
                continue;
            };
            // The proof is kept only beside the bytes it vouches for.
            if self.transport.seed(&spec, &answer, context) {
                self.transport.seed(
                    &super::links::proof_spec(&spec),
                    &super::links::proof_page(&oid),
                    context,
                );
            }
        }
    }
}

impl FilesAtCommit<'_> {
    fn request(&self, path: &str) -> ContentRequest {
        ContentRequest {
            owner: self.owner.to_owned(),
            repo: self.repo.to_owned(),
            path: path.to_owned(),
            reference: Some(self.commit.to_owned()),
        }
    }
}
