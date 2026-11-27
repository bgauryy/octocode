//! `cochange`: the files Git changed in the same non-merge commits as a
//! target file, an overlay read from history at query time and never stored
//! in the snapshot.
//!
//! One `git log` lists the target's commits with every file each changed
//! (`--full-diff`), so the cost follows the target's commits, never all pairs
//! of files. `--relative` keeps the files under the scope root; renames are
//! not followed. Neither history nor commit size is capped: the rows page.
//!
//! A shallow clone's boundary commits (Git's `shallow` file) have no parents
//! in the repository, so Git diffs them against nothing and lists their
//! whole tree: no change set. They count toward nothing, and the answer
//! warns that the history is shallow.
use super::assess::git;
use super::*;
use std::collections::{HashMap, HashSet};

impl Query<'_> {
    pub(super) fn cochange(&self) -> Result<GraphOutput, GraphOutput> {
        let (extra, page) = self.complete_rows(|| self.cochange_rows())?;
        Ok(run_page(self.options, self.g, extra, page))
    }

    /// Every co-changed file, most shared commits first, and the target's
    /// commit count the first page states.
    fn cochange_rows(&self) -> Result<(Map<String, Value>, Vec<Value>), GraphOutput> {
        let g = self.g;
        let target = g.file_of(self.target_of_kind("a file", Some(NodeKind::File))?);
        if g.t.nodes[target as usize].kind != NodeKind::File {
            return Err(GraphOutput::error(
                ErrorCode::InvalidInput,
                format!("`cochange` needs a file; {:?} is a package", g.key(target)),
            ));
        }
        let key = g.key(target);
        let root = PathBuf::from(g.manifest["root"].as_str().unwrap_or_default());
        let log = git(
            &root,
            &[
                "--literal-pathspecs",
                "log",
                "--no-merges",
                "--full-history",
                "--full-diff",
                "--no-renames",
                "--relative",
                "--name-only",
                "-z",
                "--format=%x00%H",
                "--",
                key,
            ],
            self.cancel,
        )
        .map_err(|failure| self.history_failure(failure))?;
        let boundaries = shallow_boundaries(&root, self.cancel)?;
        let (target_commits, skipped, counts) =
            shared_commits(&String::from_utf8_lossy(&log), key, &boundaries);
        let linked = linked_files(g, target);
        let mut rows = counts
            .into_iter()
            .map(|(path, commits)| {
                let joined = g.t.by_key(&path).is_some_and(|id| linked.contains(&id));
                (path, commits, joined)
            })
            .collect::<Vec<_>>();
        // With one denominator, share orders as commits do.
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let rows = rows
            .into_iter()
            .map(|(path, commits, joined)| {
                json!({
                    "path": path,
                    "commits": commits,
                    "share": share(commits, target_commits),
                    "linked": joined,
                })
            })
            .collect();
        let mut summary = json!({"targetCommits": target_commits});
        if target_commits == 0 {
            summary["reason"] = json!(if skipped > 0 {
                format!(
                    "Git's shallow history holds only boundary commits that changed {key}: their parents are missing, so they show no change set"
                )
            } else {
                format!(
                    "Git has no non-merge commit that changed {key}: it is untracked, or changed only by merges"
                )
            });
        }
        let mut extra = Map::new();
        if !boundaries.is_empty() {
            extra.insert(
                "warnings".into(),
                json!([format!(
                    "Git history under {} is shallow: co-change counts only the commits it holds{}. Deepen it (`git fetch --deepen=<n>` or `git fetch --unshallow`) to count older commits.",
                    g.shown_root(),
                    if skipped > 0 {
                        format!(", and the {skipped} boundary commit(s) that list {key} count toward nothing (their parents are missing, so Git shows their whole tree)")
                    } else {
                        String::new()
                    }
                )]),
            );
        }
        extra.insert(
            "target".into(),
            Value::Object(g.node_json(target, self.cancel)?),
        );
        extra.insert("summary".into(), summary);
        Ok((extra, rows))
    }

    /// A failed history read, restated so the reader knows the overlay
    /// needs Git under the snapshot's root.
    fn history_failure(&self, mut failure: GraphOutput) -> GraphOutput {
        if failure.value["errorCode"] == json!(ErrorCode::GitFailed) {
            let reason = failure.value["error"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            failure.value["error"] = json!(format!(
                "cochange reads Git history, and Git could not read it under {}: {reason}. Build the graph for a path inside a Git work tree.",
                self.g.shown_root()
            ));
        }
        failure
    }
}

/// The boundary commits of a shallow repository, as Git's `shallow` file
/// lists them; none when the history is whole (no such file).
fn shallow_boundaries(
    root: &Path,
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
) -> Result<HashSet<String>, GraphOutput> {
    // Git before 2.31 has no `--path-format`, so the path is as printed:
    // absolute, or relative to the directory Git ran in.
    let printed = git(root, &["rev-parse", "--git-path", "shallow"], cancel)?;
    let path = printed_git_path(root, &String::from_utf8_lossy(&printed));
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::disallowed_methods,
            reason = "Git's own file of boundary commit hashes, one line each, sized by the clone's depth: a cap would cut valid history, so it is read whole"
        )
    )]
    let read = std::fs::read_to_string(&path);
    match read {
        Ok(text) => Ok(text
            .lines()
            .map(str::trim)
            .filter(|hash| !hash.is_empty())
            .map(str::to_owned)
            .collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashSet::new()),
        Err(error) => Err(GraphOutput::error(
            ErrorCode::GitFailed,
            format!("cannot read Git's shallow file {}: {error}", path.display()),
        )),
    }
}

/// The file `git -C root rev-parse --git-path` printed: absolute, or relative
/// to `root`, where Git ran.
fn printed_git_path(root: &Path, printed: &str) -> PathBuf {
    root.join(printed.trim_end_matches(['\n', '\r']))
}

/// From `git log -z --name-only --format=%x00%H` output: the commits it
/// lists other than `boundaries`, how many of `boundaries` it listed, and
/// for each other file how many of the counted commits changed it. A commit
/// begins after an empty field (its hash follows); every other field is a
/// path, the first of a commit preceded by the newline ending its header.
fn shared_commits(
    log: &str,
    target: &str,
    boundaries: &HashSet<String>,
) -> (usize, usize, HashMap<String, usize>) {
    let (mut commits, mut skipped) = (0, 0);
    let mut counts = HashMap::<String, usize>::new();
    let mut fields = log.split('\0');
    let mut first_path = false;
    let mut counted = false;
    while let Some(field) = fields.next() {
        if field.is_empty() {
            if let Some(hash) = fields.next().filter(|hash| !hash.is_empty()) {
                counted = !boundaries.contains(hash);
                if counted {
                    commits += 1;
                } else {
                    skipped += 1;
                }
                first_path = true;
            }
            continue;
        }
        let path = if first_path {
            field.strip_prefix('\n').unwrap_or(field)
        } else {
            field
        };
        first_path = false;
        if counted && path != target {
            *counts.entry(path.to_owned()).or_default() += 1;
        }
    }
    (commits, skipped, counts)
}

/// `commits` over the target's commits, to three decimals.
fn share(commits: usize, target_commits: usize) -> f64 {
    if target_commits == 0 {
        return 0.0;
    }
    (commits as f64 / target_commits as f64 * 1000.0).round() / 1000.0
}

/// The files an edge of the snapshot joins to `file`, either way: edges of
/// the file and of every symbol it declares.
fn linked_files(g: &Graph, file: u32) -> HashSet<u32> {
    let mut nodes = descendants(g, file);
    nodes.push(file);
    let mut files = HashSet::new();
    for node in nodes {
        files.extend(g.t.out(node).iter().map(|edge| g.file_of(edge.dst)));
        files.extend(g.t.incoming(node).map(|edge| g.file_of(edge.src)));
    }
    files.remove(&file);
    files
}

#[cfg(test)]
mod tests;
