//! Bounded observations of Git state; never a complete edit/command audit.
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
use crate::store::PAGE_BYTES;

struct Git {
    deadline: Instant,
}
struct ChildGuard(Option<Child>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            #[cfg(unix)]
            {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(child.id() as i32),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Git {
    fn read(&self, cwd: &Path, args: &[&str]) -> Result<String> {
        let mut command = Command::new("git");
        command
            .current_dir(cwd)
            .args([
                "--no-pager",
                "--no-optional-locks",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.untrackedCache=false",
                "-c",
                "log.showSignature=false",
            ])
            .args(args)
            .env("GIT_NO_LAZY_FETCH", "1");
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
        ] {
            command.env_remove(key);
        }
        self.execute(command)
    }
    fn execute(&self, mut command: Command) -> Result<String> {
        let mut out = tempfile::tempfile()?;
        let mut err = tempfile::tempfile()?;
        command
            .stdin(Stdio::null())
            .stdout(out.try_clone()?)
            .stderr(err.try_clone()?);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut guard = ChildGuard(Some(
            command.spawn().context("Git is required for activity")?,
        ));
        loop {
            if out.metadata()?.len() > MAX_BYTES || err.metadata()?.len() > MAX_BYTES {
                bail!(
                    "Git activity exceeds 8 MiB; use a smaller workspace for files or lower scanLimit for history"
                );
            }
            if let Some(status) = guard
                .0
                .as_mut()
                .ok_or_else(|| anyhow!("Git process missing"))?
                .try_wait()?
            {
                guard.0.take();
                let stdout = read_file(&mut out)?;
                if !status.success() {
                    bail!(
                        "Git activity failed: {}",
                        read_file(&mut err)?.chars().take(2000).collect::<String>()
                    );
                }
                return Ok(stdout);
            }
            if Instant::now() >= self.deadline {
                bail!(
                    "Git activity exceeded 5 seconds; use a smaller workspace for files or lower scanLimit for history"
                );
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
fn read_file(file: &mut File) -> Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut value = String::new();
    file.take(MAX_BYTES + 1)
        .read_to_string(&mut value)
        .context("Git activity requires UTF-8 paths and metadata")?;
    if value.len() as u64 > MAX_BYTES {
        bail!("Git activity exceeds 8 MiB");
    }
    Ok(value)
}

struct Filter {
    prefix: String,
    regex: Option<jsonschema::Validator>,
    since: Option<i64>,
    until: Option<i64>,
}
impl Filter {
    fn new(input: &Value, now: i64) -> Result<Self> {
        let raw_prefix = input["path"].as_str().unwrap_or("");
        let prefix = raw_prefix.trim_end_matches('/').to_owned();
        if raw_prefix.starts_with('/')
            || raw_prefix.split('/').any(|p| p == "." || p == "..")
            || prefix.contains('\\')
            || Path::new(&prefix)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            bail!("path must be a workspace-relative slash-separated prefix without . or ..");
        }
        let regex = input["pathRegex"]
            .as_str()
            .map(|pattern| {
                // Reuse the existing schema engine's bounded linear-time regex matcher.
                jsonschema::options()
                    .with_pattern_options(
                        jsonschema::PatternOptions::regex()
                            .size_limit(1024 * 1024)
                            .dfa_size_limit(1024 * 1024),
                    )
                    .build(&json!({"type":"string","pattern":pattern}))
                    .map_err(|e| anyhow!("Invalid pathRegex: {e}"))
            })
            .transpose()?;
        let since = input["sinceMs"].as_i64().or_else(|| {
            input["withinMs"]
                .as_i64()
                .map(|v| now.saturating_sub(v).max(0))
        });
        let until = input["untilMs"].as_i64();
        if since.zip(until).is_some_and(|(s, u)| s > u) {
            bail!("sinceMs must not exceed untilMs");
        }
        Ok(Self {
            prefix,
            regex,
            since,
            until,
        })
    }
    fn path(&self, path: &str) -> bool {
        (self.prefix.is_empty()
            || path == self.prefix
            || path
                .strip_prefix(&self.prefix)
                .is_some_and(|p| p.starts_with('/')))
            && self.regex.as_ref().is_none_or(|r| r.is_valid(&json!(path)))
    }
    fn time(&self, at: Option<i64>) -> bool {
        match at {
            Some(at) => self.since.is_none_or(|s| at >= s) && self.until.is_none_or(|u| at <= u),
            None => self.since.is_none() && self.until.is_none(),
        }
    }
}
fn relative<'a>(path: &'a str, scope: &str) -> Option<&'a str> {
    if scope.is_empty() {
        Some(path)
    } else {
        path.strip_prefix(scope)?.strip_prefix('/')
    }
}
fn modified(workspace: &Path, path: &str) -> Option<i64> {
    let full = workspace.join(path);
    // Do not follow ancestor symlinks outside the workspace, or the final link itself.
    if !full.parent()?.canonicalize().ok()?.starts_with(workspace) {
        return None;
    }
    i64::try_from(
        full.symlink_metadata()
            .ok()?
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis(),
    )
    .ok()
}
fn change_state(code: u8) -> &'static str {
    match code {
        b' ' => "unchanged",
        b'M' => "modified",
        b'A' => "added",
        b'D' => "deleted",
        b'R' => "renamed",
        b'C' => "copied",
        b'T' => "type-changed",
        b'U' => "unmerged",
        b'?' => "untracked",
        b'!' => "ignored",
        _ => "unknown",
    }
}
fn files(
    git: &Git,
    workspace: &Path,
    scope: &str,
    filter: &Filter,
) -> Result<(Vec<Value>, usize, usize)> {
    let raw = git.read(
        workspace,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ],
    )?;
    let mut fields = raw.split_terminator('\0');
    let mut rows = Vec::new();
    let mut unknown = 0;
    let mut scanned = 0;
    while let Some(record) = fields.next() {
        if record.len() < 4 || record.as_bytes()[2] != b' ' {
            bail!("Invalid Git status record");
        }
        let status = record
            .get(..2)
            .ok_or_else(|| anyhow!("Invalid Git status"))?;
        let original = if status.contains(['R', 'C']) {
            Some(
                fields
                    .next()
                    .ok_or_else(|| anyhow!("Missing rename source"))?,
            )
        } else {
            None
        };
        let Some(path) = relative(&record[3..], scope) else {
            continue;
        };
        scanned += 1;
        let original = original.and_then(|p| relative(p, scope));
        if !filter.path(path) && !original.is_some_and(|p| filter.path(p)) {
            continue;
        }
        let at = modified(workspace, path);
        if at.is_none() {
            unknown += 1;
        }
        if filter.time(at) {
            let index_state = change_state(status.as_bytes()[0]);
            let worktree_state = change_state(status.as_bytes()[1]);
            let mut changes = Vec::new();
            for state in [index_state, worktree_state] {
                if state != "unchanged" && !changes.contains(&state) {
                    changes.push(state);
                }
            }
            rows.push(json!({"path":path,"status":status,"changes":changes,"indexState":index_state,"worktreeState":worktree_state,"previousPath":original,"modifiedAt":at}));
        }
    }
    rows.sort_by(|a, b| {
        b["modifiedAt"]
            .as_i64()
            .cmp(&a["modifiedAt"].as_i64())
            .then_with(|| a["path"].as_str().cmp(&b["path"].as_str()))
    });
    Ok((rows, unknown, scanned))
}
fn epoch(value: &str) -> Result<i64> {
    value
        .parse::<i64>()?
        .checked_mul(1000)
        .ok_or_else(|| anyhow!("Git timestamp overflow"))
}
fn commits(raw: &str, scope: &str, filter: &Filter) -> Result<Vec<Value>> {
    let mut fields = raw.split_terminator('\0').peekable();
    let mut rows = Vec::new();
    while fields.next().is_some() {
        let hash = fields
            .next()
            .ok_or_else(|| anyhow!("Missing commit hash"))?;
        let at = epoch(
            fields
                .next()
                .ok_or_else(|| anyhow!("Missing commit time"))?,
        )?;
        let subject = fields
            .next()
            .ok_or_else(|| anyhow!("Missing commit subject"))?;
        let mut paths = Vec::new();
        let mut first = true;
        while fields.peek().is_some_and(|v| !v.is_empty()) {
            let path = fields
                .next()
                .ok_or_else(|| anyhow!("Missing commit path"))?;
            let path = if first {
                path.strip_prefix('\n').unwrap_or(path)
            } else {
                path
            };
            first = false;
            if let Some(path) = relative(path, scope).filter(|p| filter.path(p)) {
                paths.push(path.to_owned());
            }
        }
        // Keep all scanned commits for truthful coverage and cursor snapshots.
        let path_match = !paths.is_empty()
            || (scope.is_empty() && filter.prefix.is_empty() && filter.regex.is_none());
        rows.push(json!({"hash":hash,"at":at,"subject":subject.chars().take(256).collect::<String>(),"subjectTruncated":subject.chars().count()>256,"paths":paths.iter().take(20).collect::<Vec<_>>(),"matchedPathCount":paths.len(),"pathsTruncated":paths.len()>20,"matches":path_match && filter.time(Some(at))}));
    }
    Ok(rows)
}
fn reflog(raw: &str, filter: &Filter) -> Result<Vec<Value>> {
    let fields: Vec<_> = raw.split_terminator('\0').collect();
    if !fields.len().is_multiple_of(3) {
        bail!("Invalid reflog output");
    }
    fields.chunks(3).map(|row| {
        let seconds = row[1].rsplit_once("@{").and_then(|(_,s)| s.strip_suffix('}')).ok_or_else(|| anyhow!("Missing reflog event time"))?;
        let at = epoch(seconds)?;
        Ok(json!({"hash":row[0],"at":at,"action":row[2].chars().take(256).collect::<String>(),"actionTruncated":row[2].chars().count()>256,"matches":filter.time(Some(at))}))
    }).collect()
}

/// Input is validated by its callers: the CLI ingress and `Store::call`.
pub(crate) fn read(workspace: &Path, input: &Value) -> Result<Value> {
    let now = crate::store::now();
    let filter = Filter::new(input, now)?;
    let view = input["view"].as_str().unwrap_or("files");
    if view == "reflog" && (input.get("path").is_some() || input.get("pathRegex").is_some()) {
        bail!("Reflog events do not identify changed paths; use view:commits for path filters");
    }
    let workspace = workspace.canonicalize()?;
    let git = Git {
        deadline: Instant::now() + Duration::from_secs(5),
    };
    let root = git.read(&workspace, &["rev-parse", "--show-toplevel"])?;
    let root = Path::new(root.strip_suffix('\n').unwrap_or(&root)).canonicalize()?;
    let scope = workspace
        .strip_prefix(&root)?
        .to_str()
        .ok_or_else(|| anyhow!("Workspace must be UTF-8"))?
        .replace('\\', "/");
    let cap = input["scanLimit"].as_u64().unwrap_or(200) as usize;
    let mut unknown = 0;
    let (mut rows, scanned, truncated) = if view == "files" {
        let (rows, count, scanned) = files(&git, &workspace, &scope, &filter)?;
        unknown = count;
        (rows, scanned, false)
    } else {
        let limit = format!("-{}", cap + 1);
        let has_head = !git
            .read(&workspace, &["rev-parse", "--revs-only", "HEAD"])?
            .trim()
            .is_empty();
        let raw = if !has_head {
            String::new()
        } else if view == "commits" {
            git.read(
                &workspace,
                &[
                    "log",
                    &limit,
                    "--date-order",
                    "-z",
                    "--name-only",
                    "--format=%x00%H%x00%ct%x00%s",
                    "--no-renames",
                    "--no-ext-diff",
                    "--diff-merges=first-parent",
                    "--full-history",
                ],
            )?
        } else {
            git.read(
                &workspace,
                &[
                    "reflog",
                    "show",
                    &limit,
                    "-z",
                    "--date=unix",
                    "--format=%H%x00%gD%x00%gs",
                    "HEAD",
                ],
            )?
        };
        let mut rows = if view == "commits" {
            commits(&raw, &scope, &filter)?
        } else {
            reflog(&raw, &filter)?
        };
        let truncated = rows.len() > cap;
        rows.truncate(cap);
        let scanned = rows.len();
        rows.retain(|r| r["matches"] == true);
        for row in &mut rows {
            row.as_object_mut()
                .ok_or_else(|| anyhow!("Invalid activity row"))?
                .remove("matches");
        }
        rows.sort_by(|a, b| b["at"].as_i64().cmp(&a["at"].as_i64()));
        (rows, scanned, truncated)
    };
    let digest: String = Sha256::digest(serde_json::to_vec(&rows)?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if input["snapshot"].as_str().is_some_and(|s| s != digest) {
        bail!("Activity changed during pagination; restart without after/snapshot");
    }
    let after = input["after"].as_u64().unwrap_or(0) as usize;
    if after > rows.len() {
        bail!("Activity cursor exceeds result count");
    }
    let count = input["limit"].as_u64().unwrap_or(20) as usize;
    let total = rows.len();
    let mut end = after;
    let mut bytes = 0;
    while end < total && end - after < count {
        let size = serde_json::to_vec(&rows[end])?.len();
        if end > after && bytes + size > PAGE_BYTES {
            break;
        }
        bytes += size;
        end += 1;
    }
    let next = if end < total {
        let mut next = input.clone();
        next.as_object_mut()
            .ok_or_else(|| anyhow!("Expected object"))?
            .remove("withinMs");
        if let Some(since) = filter.since {
            next["sinceMs"] = json!(since);
        }
        next["after"] = json!(end);
        next["snapshot"] = json!(digest);
        json!({"command":"activity","input":next})
    } else {
        Value::Null
    };
    let unknown_time_query = if unknown > 0
        && ["withinMs", "sinceMs", "untilMs"]
            .iter()
            .any(|key| input.get(*key).is_some())
    {
        let mut query = input.clone();
        if let Some(fields) = query.as_object_mut() {
            for key in ["withinMs", "sinceMs", "untilMs", "after", "snapshot"] {
                fields.remove(key);
            }
        }
        query
    } else {
        Value::Null
    };
    let mut result = json!({"view":view,"workspace":workspace,"observedAt":now,"items":rows.drain(after..end).collect::<Vec<_>>(),"totalMatched":total,"next":next,"coverage":{"scanned":scanned,"scanLimit":if view=="files" {Value::Null}else{json!(cap)},"truncated":truncated,"limitReason":if truncated {json!("History scan limit reached; increase scanLimit (max 2000). This is not complete history.")}else{Value::Null},"unknownFileTimes":unknown,"unknownTimeQuery":unknown_time_query},"timeBasis":match view {"files"=>"Current filesystem mtime; not an edit/staging timestamp. Deletions may have unknown time and are excluded by time filters.","commits"=>"Committer time on current HEAD history; not command execution time.",_=>"Local HEAD reflog time; reference updates only, not complete Git command history."},"attribution":"Observed Git/filesystem state does not identify the acting agent."});
    if bytes > PAGE_BYTES {
        result["budget"] = json!({"targetBytes":PAGE_BYTES,"reason":"Single oversized row returned intact to preserve evidence and cursor progress"});
    }
    Ok(result)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn output_limit_stops_the_process() -> Result<()> {
        let git = Git {
            deadline: Instant::now() + Duration::from_secs(5),
        };
        let mut command = Command::new("/bin/dd");
        command.args(["if=/dev/zero", "bs=1048576", "count=9"]);
        assert!(
            git.execute(command)
                .err()
                .context("Expected process limit error")?
                .to_string()
                .contains("exceeds 8 MiB")
        );
        Ok(())
    }

    #[test]
    fn deadline_kills_and_reaps_the_process() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let pid_file = directory.path().join("pid");
        let git = Git {
            deadline: Instant::now() + Duration::from_secs(5),
        };
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "echo $$ > \"$1\"; exec /bin/sleep 30",
                "activity-test",
            ])
            .arg(&pid_file);
        assert!(
            git.execute(command)
                .err()
                .context("Expected process limit error")?
                .to_string()
                .contains("exceeded 5 seconds")
        );
        let pid = std::fs::read_to_string(pid_file)?.trim().parse::<i32>()?;
        assert_eq!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
            Err(nix::errno::Errno::ESRCH)
        );
        Ok(())
    }
}
