//! Transaction journal: write-ahead log, commit, and crash-recovery for applied rewrites.
use super::{
    FileState, JOURNAL_PREFIX, Journal, JournalFile, JournalPhase, PreparedFile, RewriteError,
    cancelled, create_private_dir_all, io_error, sha256, transaction_id,
};
use crate::tools::cancel::CancellationCheck;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub(super) fn persist_journal(path: &Path, journal: &Journal) -> Result<(), RewriteError> {
    let bytes = serde_json::to_vec(journal)
        .map_err(|error| RewriteError::new("transactionFailed", error.to_string()))?;
    crate::private_file::write_atomic(path, &bytes, true).map_err(io_error)?;
    Ok(())
}

fn sync_path(path: &Path) -> Result<(), RewriteError> {
    OpenOptions::new()
        .read(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(io_error)
}

pub(super) fn journal_directory(boundary: &Path) -> PathBuf {
    super::state_base_dir()
        .join(format!(
            "octocode-ast-rewrite-transactions-v1-{}",
            super::state_dir_uid_suffix()
        ))
        .join(sha256(boundary.to_string_lossy().as_bytes()))
}

pub(super) fn commit_transaction(
    boundary: &Path,
    files: &[PreparedFile],
    cancellation: &dyn CancellationCheck,
) -> Result<Value, RewriteError> {
    let id = transaction_id(boundary, files);
    let journal_dir = journal_directory(boundary);
    create_private_dir_all(&journal_dir)?;
    let journal_path = journal_dir.join(format!("{JOURNAL_PREFIX}{id}.json"));
    let mut journal = plan_journal(boundary, files, &id);
    persist_journal(&journal_path, &journal)?;
    let result = stage_files(&mut journal, &journal_path, files, cancellation)
        .and_then(|()| verify_targets(files))
        .and_then(|()| promote_files(&mut journal, &journal_path, files, cancellation));
    match result {
        Ok(()) => {
            let Settled { errors, warnings } = finalize_committed(&journal_path, &journal);
            let warnings = errors.into_iter().chain(warnings).collect::<Vec<_>>();
            // Per-file before/after hashes live on the result's `files[]`.
            let mut receipt = json!({"id":id,"committed":true,"files":files.len()});
            if !warnings.is_empty() {
                receipt["cleanupWarnings"] = json!(warnings);
            }
            Ok(receipt)
        }
        Err(error) => {
            let Settled {
                errors: recovered,
                warnings,
            } = recover_one(&journal_path, &journal);
            Err(RewriteError::new(
                "transactionFailed",
                if recovered.is_empty() {
                    "The rewrite transaction failed; automatic recovery completed."
                } else {
                    "The rewrite transaction failed; automatic recovery is incomplete."
                },
            )
            .detail(json!({
                "cause":error.message,
                "rollback":{"restored":recovered.is_empty(),"files":files.len(),"errors":recovered,"warnings":warnings}
            })))
        }
    }
}

/// The journal before any write: each target with its stage and backup
/// siblings and its expected hashes.
fn plan_journal(boundary: &Path, files: &[PreparedFile], id: &str) -> Journal {
    Journal {
        version: 1,
        id: id.to_owned(),
        root: boundary.to_path_buf(),
        phase: JournalPhase::Staging,
        files: files
            .iter()
            .enumerate()
            .map(|(index, file)| JournalFile {
                target: file.absolute.clone(),
                stage: file
                    .absolute
                    .with_file_name(format!(".octocode-{id}.stage-{index}")),
                backup: file
                    .absolute
                    .with_file_name(format!(".octocode-{id}.backup-{index}")),
                before_hash: file.before_hash.clone(),
                after_hash: file.after_hash.clone(),
                state: FileState::Planned,
            })
            .collect(),
    }
}

/// Writes every new content beside its target into an owner-only stage,
/// synced, then gives the stage the target's permissions.
fn stage_files(
    journal: &mut Journal,
    journal_path: &Path,
    files: &[PreparedFile],
    cancellation: &dyn CancellationCheck,
) -> Result<(), RewriteError> {
    for (index, file) in files.iter().enumerate() {
        cancellation.check().map_err(cancelled)?;
        let journal_file = &journal.files[index];
        let mut stage = create_stage(&journal_file.stage, &file.permissions).map_err(io_error)?;
        stage.write_all(&file.after).map_err(io_error)?;
        stage.sync_all().map_err(io_error)?;
        fs::set_permissions(&journal_file.stage, file.permissions.clone()).map_err(io_error)?;
        journal.files[index].state = FileState::Staged;
        persist_journal(journal_path, journal)?;
    }
    journal.phase = JournalPhase::Prepared;
    persist_journal(journal_path, journal)
}

/// A new, empty stage file, owner-only and no wider than its source before
/// its first byte.
fn create_stage(path: &Path, source: &fs::Permissions) -> std::io::Result<fs::File> {
    #[cfg(unix)]
    let mode = std::os::unix::fs::PermissionsExt::mode(source);
    #[cfg(not(unix))]
    let mode = {
        let _ = source;
        0o600
    };
    crate::private_file::create_new(path, mode)
}

/// Every target is still the regular file the preview hashed.
fn verify_targets(files: &[PreparedFile]) -> Result<(), RewriteError> {
    for file in files {
        let metadata = fs::symlink_metadata(&file.absolute).map_err(io_error)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || sha256(&fs::read(&file.absolute).map_err(io_error)?) != file.before_hash
        {
            return Err(RewriteError::new(
                "transactionFailed",
                format!("Target bytes changed: {}", file.absolute.display()),
            ));
        }
    }
    Ok(())
}

/// Backs up each target and renames its stage into place, then syncs the
/// directories.
fn promote_files(
    journal: &mut Journal,
    journal_path: &Path,
    files: &[PreparedFile],
    cancellation: &dyn CancellationCheck,
) -> Result<(), RewriteError> {
    journal.phase = JournalPhase::Committing;
    persist_journal(journal_path, journal)?;
    for index in 0..journal.files.len() {
        cancellation.check().map_err(cancelled)?;
        let journal_file = &journal.files[index];
        fs::rename(&journal_file.target, &journal_file.backup).map_err(io_error)?;
        journal.files[index].state = FileState::BackedUp;
        persist_journal(journal_path, journal)?;
        if sha256(&fs::read(&journal.files[index].backup).map_err(io_error)?)
            != journal.files[index].before_hash
        {
            return Err(RewriteError::new(
                "transactionFailed",
                "Target changed during commit.",
            ));
        }
        fs::rename(&journal.files[index].stage, &journal.files[index].target).map_err(io_error)?;
        sync_path(&journal.files[index].target)?;
        journal.files[index].state = FileState::Promoted;
        persist_journal(journal_path, journal)?;
    }
    // Windows cannot open a directory to sync it; there the renames rest on
    // the filesystem's own ordering. A failed sync elsewhere fails the
    // transaction, which recovery rolls back from the backups.
    #[cfg(unix)]
    for directory in files
        .iter()
        .filter_map(|file| file.absolute.parent())
        .collect::<BTreeSet<_>>()
    {
        sync_path(directory)?;
    }
    journal.phase = JournalPhase::Committed;
    persist_journal(journal_path, journal)
}

/// Settles every interrupted transaction on `boundary`. `Ok` carries the
/// warnings of transactions that settled while keeping an external edit.
/// Settles every interrupted transaction the caller's root lock covers: the
/// boundary's own journals and those of roots nested inside it (the lock
/// rejects overlapping roots). A pending journal of an enclosing root is
/// left to that root's lock and named in a warning.
pub(super) fn recover_transactions(
    boundary: &Path,
    cancellation: &dyn CancellationCheck,
) -> Result<Vec<String>, RewriteError> {
    let own = journal_directory(boundary);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for directory in journal_directories(&own)? {
        cancellation.check().map_err(cancelled)?;
        for (path, journal) in read_journals(&directory, &mut errors)? {
            cancellation.check().map_err(cancelled)?;
            let root = if directory == own {
                boundary
            } else if journal.root != boundary
                && journal.root.starts_with(boundary)
                && directory == journal_directory(&journal.root)
            {
                journal.root.as_path()
            } else {
                if journal.root != boundary && boundary.starts_with(&journal.root) {
                    warnings.push(format!(
                        "An interrupted astRewrite transaction on the enclosing root {} is pending; apply a rewrite on that root to recover it.",
                        journal.root.display()
                    ));
                }
                continue;
            };
            if !valid_journal(root, &journal) {
                errors.push(format!("Invalid transaction journal: {}", path.display()));
                continue;
            }
            let settled = if journal.phase == JournalPhase::Committed {
                finalize_committed(&path, &journal)
            } else {
                recover_one(&path, &journal)
            };
            errors.extend(settled.errors);
            warnings.extend(settled.warnings);
        }
    }
    if errors.is_empty() {
        Ok(warnings)
    } else {
        Err(RewriteError::new(
            "recoveryFailed",
            "An interrupted astRewrite transaction could not be recovered safely.",
        )
        .detail(json!({"errors":errors})))
    }
}

/// `own` first, then every other per-root journal directory.
fn journal_directories(own: &Path) -> Result<Vec<PathBuf>, RewriteError> {
    let mut directories = vec![own.to_path_buf()];
    let Some(base) = own.parent() else {
        return Ok(directories);
    };
    let entries = match fs::read_dir(base) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(directories),
        Err(error) => return Err(io_error(error)),
    };
    for entry in entries {
        let entry = entry.map_err(io_error)?;
        if entry.file_type().map_err(io_error)?.is_dir() && entry.path() != own {
            directories.push(entry.path());
        }
    }
    Ok(directories)
}

/// The journals in `directory`; an unreadable one is recorded in `errors`.
fn read_journals(
    directory: &Path,
    errors: &mut Vec<String>,
) -> Result<Vec<(PathBuf, Journal)>, RewriteError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error(error)),
    }
    .collect::<Result<Vec<_>, _>>()
    .map_err(io_error)?;
    let mut journals = Vec::new();
    for entry in entries {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(JOURNAL_PREFIX) || !name.ends_with(".json") {
            continue;
        }
        let path = entry.path();
        match fs::read(&path).map_err(io_error).and_then(|bytes| {
            serde_json::from_slice::<Journal>(&bytes)
                .map_err(|error| RewriteError::new("recoveryFailed", error.to_string()))
        }) {
            Ok(journal) => journals.push((path, journal)),
            Err(error) => errors.push(error.message),
        }
    }
    Ok(journals)
}

fn valid_journal(boundary: &Path, journal: &Journal) -> bool {
    journal.version == 1
        && journal.root == boundary
        && journal.files.iter().enumerate().all(|(index, file)| {
            file.target.starts_with(boundary)
                && file.target.parent() == file.stage.parent()
                && file.target.parent() == file.backup.parent()
                && file.stage.file_name().is_some_and(|name| {
                    name == std::ffi::OsStr::new(&format!(".octocode-{}.stage-{index}", journal.id))
                })
                && file.backup.file_name().is_some_and(|name| {
                    name == std::ffi::OsStr::new(&format!(
                        ".octocode-{}.backup-{index}",
                        journal.id
                    ))
                })
                && file.before_hash.len() == 64
                && file.after_hash.len() == 64
        })
}

fn hash_at(path: &Path) -> Result<Option<String>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(sha256(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// The outcome of settling one journal: errors keep the journal; warnings
/// report an external edit that was kept while the journal retired.
#[derive(Default)]
pub(super) struct Settled {
    pub(super) errors: Vec<String>,
    pub(super) warnings: Vec<String>,
}

fn recover_one(path: &Path, journal: &Journal) -> Settled {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for file in journal.files.iter().rev() {
        let result = (|| -> Result<(), String> {
            let target_hash = hash_at(&file.target)?;
            let backup_hash = hash_at(&file.backup)?;
            if let Some(backup_hash) = backup_hash {
                if backup_hash != file.before_hash {
                    if target_hash.is_none() {
                        fs::rename(&file.backup, &file.target)
                            .map_err(|error| error.to_string())?;
                        if file.stage.exists() {
                            fs::remove_file(&file.stage).map_err(|error| error.to_string())?;
                        }
                        return Ok(());
                    }
                    return Err(format!("Backup hash mismatch: {}", file.target.display()));
                }
                if target_hash
                    .as_ref()
                    .is_some_and(|hash| hash != &file.before_hash && hash != &file.after_hash)
                {
                    return Err(format!(
                        "External target change blocks recovery: {}",
                        file.target.display()
                    ));
                }
                if target_hash.as_deref() == Some(file.before_hash.as_str()) {
                    fs::remove_file(&file.backup).map_err(|error| error.to_string())?;
                } else {
                    if file.target.exists() {
                        fs::remove_file(&file.target).map_err(|error| error.to_string())?;
                    }
                    fs::rename(&file.backup, &file.target).map_err(|error| error.to_string())?;
                }
            } else if target_hash.as_deref() != Some(file.before_hash.as_str()) {
                // The backup rename is persisted (BackedUp) only after it
                // happens, so a Planned/Staged target with no backup was never
                // moved: its current bytes are someone else's edit. Keep them.
                if !matches!(file.state, FileState::Planned | FileState::Staged) {
                    return Err(format!(
                        "Original bytes unavailable for recovery: {}",
                        file.target.display()
                    ));
                }
                warnings.push(format!(
                    "Target changed outside the transaction before it was moved; kept its current bytes: {}",
                    file.target.display()
                ));
            }
            if file.stage.exists() {
                fs::remove_file(&file.stage).map_err(|error| error.to_string())?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            errors.push(error);
        }
    }
    Settled {
        errors: retire(path, errors),
        warnings,
    }
}

/// Once every file settled cleanly, delete the journal and its directory.
fn retire(path: &Path, mut errors: Vec<String>) -> Vec<String> {
    if errors.is_empty()
        && let Err(error) = fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        errors.push(error.to_string());
    }
    if errors.is_empty()
        && let Some(parent) = path.parent()
    {
        let _ = fs::remove_dir(parent);
    }
    errors
}

/// Removes a committed transaction's stage and backup files. A target edited
/// after the commit keeps its edit: the commit already happened, so the
/// mismatch is a warning and the journal still retires.
fn finalize_committed(path: &Path, journal: &Journal) -> Settled {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for file in &journal.files {
        match hash_at(&file.target) {
            Ok(Some(hash)) if hash == file.after_hash => {}
            Ok(_) => warnings.push(format!(
                "Committed target changed after the commit; kept its current bytes: {}",
                file.target.display()
            )),
            Err(error) => {
                errors.push(error);
                continue;
            }
        }
        for artifact in [&file.stage, &file.backup] {
            if artifact.exists()
                && let Err(error) = fs::remove_file(artifact)
            {
                errors.push(error.to_string());
            }
        }
    }
    Settled {
        errors: retire(path, errors),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Journals on disk keep the phase and state words earlier releases
    /// wrote, so an interrupted transaction from one stays recoverable.
    #[test]
    fn journal_phase_and_state_keep_their_on_disk_words() {
        assert_eq!(
            [
                JournalPhase::Staging,
                JournalPhase::Prepared,
                JournalPhase::Committing,
                JournalPhase::Committed,
            ]
            .map(|phase| serde_json::to_value(phase).unwrap()),
            ["staging", "prepared", "committing", "committed"].map(serde_json::Value::from)
        );
        assert_eq!(
            [
                FileState::Planned,
                FileState::Staged,
                FileState::BackedUp,
                FileState::Promoted,
            ]
            .map(|state| serde_json::to_value(state).unwrap()),
            ["planned", "staged", "backed-up", "promoted"].map(serde_json::Value::from)
        );
    }

    /// A stage holds the source's new bytes, so before its first byte it is
    /// already no wider than the source and never wider than owner-only.
    #[cfg(unix)]
    #[test]
    fn a_stage_is_never_wider_than_its_source_before_any_byte() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        for source in [0o600, 0o644, 0o640, 0o400, 0o755] {
            let path = dir.path().join(format!("stage-{source:o}"));
            let file = create_stage(&path, &fs::Permissions::from_mode(source)).unwrap();
            let staged = file.metadata().unwrap().permissions().mode() & 0o777;
            assert_eq!(file.metadata().unwrap().len(), 0);
            assert_eq!(staged & !source, 0, "source {source:o} staged {staged:o}");
            assert_eq!(staged & !0o600, 0, "source {source:o} staged {staged:o}");
        }
    }
}
