//! `cochange`: the files Git changed in the same non-merge commits as a
//! target file, an overlay read from history at query time and never stored
//! in the snapshot.
//!
//! One `git log` lists the target's commits with every file each changed
//! (`--full-diff`), so the cost follows the target's commits, never all pairs
//! of files. `--relative` keeps the files under the scope root; renames are
//! not followed. Neither history nor commit size is capped: the rows page.
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
        let (target_commits, counts) = shared_commits(&String::from_utf8_lossy(&log), key);
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
            summary["reason"] = json!(format!(
                "Git has no non-merge commit that changed {key}: it is untracked, or changed only by merges"
            ));
        }
        let mut extra = Map::new();
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

/// From `git log -z --name-only --format=%x00%H` output: the commits it
/// lists, and for each other file how many of them changed it. A commit
/// begins after an empty field (its hash follows); every other field is a
/// path, the first of a commit preceded by the newline ending its header.
fn shared_commits(log: &str, target: &str) -> (usize, HashMap<String, usize>) {
    let mut commits = 0;
    let mut counts = HashMap::<String, usize>::new();
    let mut fields = log.split('\0');
    let mut first_path = false;
    while let Some(field) = fields.next() {
        if field.is_empty() {
            if fields.next().is_some_and(|hash| !hash.is_empty()) {
                commits += 1;
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
        if path != target {
            *counts.entry(path.to_owned()).or_default() += 1;
        }
    }
    (commits, counts)
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
mod tests {
    /// Fields of a `-z` log: each commit's hash after an empty field, its
    /// files after the header's newline; the target is not its own row.
    #[test]
    fn a_z_log_counts_commits_and_the_files_beside_the_target() {
        let log = "\0h1\0\nsrc/a.ts\0src/b c.ts\0\0h2\0\nsrc/b c.ts\0src/a.ts\0\0h3\0\nsrc/a.ts\0";
        let (commits, counts) = super::shared_commits(log, "src/a.ts");
        assert_eq!(commits, 3);
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get("src/b c.ts"), Some(&2));
        assert_eq!(super::shared_commits("", "src/a.ts").0, 0);
    }
}
