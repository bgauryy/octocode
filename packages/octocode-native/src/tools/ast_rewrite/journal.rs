//! Transaction journal: write-ahead log, commit, and crash-recovery for applied rewrites.
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use crate::tools::local_fetch::CancellationCheck;
use super::{Journal, JournalFile, PreparedFile, RewriteError, JOURNAL_PREFIX, cancelled, io_error, sha256, transaction_id};



pub(super) fn persist_journal(path: &Path, journal: &Journal) -> Result<(), RewriteError> {
    let temp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(journal)
        .map_err(|error| RewriteError::new("ast.rewrite.transaction_failed", error.to_string()))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp)
        .map_err(io_error)?;
    file.write_all(&bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(temp, path).map_err(io_error)?;
    if let Some(parent) = path.parent() {
        let _ = sync_path(parent);
    }
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
    std::env::temp_dir()
        .join("octocode-ast-rewrite-transactions-v1")
        .join(sha256(boundary.to_string_lossy().as_bytes()))
}

pub(super) fn commit_transaction(
    boundary: &Path,
    files: &[PreparedFile],
    cancellation: &dyn CancellationCheck,
) -> Result<Value, RewriteError> {
    let id = transaction_id(boundary, files);
    let journal_dir = journal_directory(boundary);
    fs::create_dir_all(&journal_dir).map_err(io_error)?;
    let journal_path = journal_dir.join(format!("{JOURNAL_PREFIX}{id}.json"));
    let mut journal = Journal {
        version: 1,
        id: id.clone(),
        root: boundary.to_path_buf(),
        phase: "staging".to_owned(),
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
                state: "planned".to_owned(),
            })
            .collect(),
    };
    persist_journal(&journal_path, &journal)?;
    let result = (|| -> Result<(), RewriteError> {
        for (index, file) in files.iter().enumerate() {
            cancellation.check().map_err(cancelled)?;
            let journal_file = &journal.files[index];
            let mut stage = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&journal_file.stage)
                .map_err(io_error)?;
            stage.write_all(&file.after).map_err(io_error)?;
            stage.sync_all().map_err(io_error)?;
            fs::set_permissions(&journal_file.stage, file.permissions.clone()).map_err(io_error)?;
            journal.files[index].state = "staged".to_owned();
            persist_journal(&journal_path, &journal)?;
        }
        journal.phase = "prepared".to_owned();
        persist_journal(&journal_path, &journal)?;
        for file in files {
            let metadata = fs::symlink_metadata(&file.absolute).map_err(io_error)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || sha256(fs::read(&file.absolute).map_err(io_error)?) != file.before_hash
            {
                return Err(RewriteError::new(
                    "ast.rewrite.transaction_failed",
                    format!("Target bytes changed: {}", file.absolute.display()),
                ));
            }
        }
        journal.phase = "committing".to_owned();
        persist_journal(&journal_path, &journal)?;
        for index in 0..journal.files.len() {
            cancellation.check().map_err(cancelled)?;
            let journal_file = &journal.files[index];
            fs::rename(&journal_file.target, &journal_file.backup).map_err(io_error)?;
            journal.files[index].state = "backed-up".to_owned();
            persist_journal(&journal_path, &journal)?;
            if sha256(fs::read(&journal.files[index].backup).map_err(io_error)?)
                != journal.files[index].before_hash
            {
                return Err(RewriteError::new(
                    "ast.rewrite.transaction_failed",
                    "Target changed during commit.",
                ));
            }
            fs::rename(&journal.files[index].stage, &journal.files[index].target)
                .map_err(io_error)?;
            sync_path(&journal.files[index].target)?;
            journal.files[index].state = "promoted".to_owned();
            persist_journal(&journal_path, &journal)?;
        }
        for directory in files
            .iter()
            .filter_map(|file| file.absolute.parent())
            .collect::<BTreeSet<_>>()
        {
            let _ = sync_path(directory);
        }
        journal.phase = "committed".to_owned();
        persist_journal(&journal_path, &journal)?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            let warnings = finalize_committed(&journal_path, &journal);
            let before = files
                .iter()
                .map(|file| {
                    (
                        file.absolute.to_string_lossy().into_owned(),
                        file.before_hash.clone(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let after = files
                .iter()
                .map(|file| {
                    (
                        file.absolute.to_string_lossy().into_owned(),
                        file.after_hash.clone(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let mut receipt = json!({
                "id":id,"committed":true,"files":files.len(),
                "beforeHashes":before,"afterHashes":after
            });
            if !warnings.is_empty() {
                receipt["cleanupWarnings"] = json!(warnings);
            }
            Ok(receipt)
        }
        Err(error) => {
            let recovered = recover_one(&journal_path, &journal);
            Err(RewriteError::new(
                "ast.rewrite.transaction_failed",
                if recovered.is_empty() {
                    "The rewrite transaction failed; automatic recovery completed."
                } else {
                    "The rewrite transaction failed; automatic recovery is incomplete."
                },
            )
            .detail(json!({
                "cause":error.message,
                "rollback":{"restored":recovered.is_empty(),"files":files.len(),"errors":recovered}
            })))
        }
    }
}

pub(super) fn recover_transactions(
    boundary: &Path,
    cancellation: &dyn CancellationCheck,
) -> Result<(), RewriteError> {
    let directory = journal_directory(boundary);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error(error)),
    }
    .collect::<Result<Vec<_>, _>>()
    .map_err(io_error)?;
    let mut errors = Vec::new();
    for entry in entries {
        cancellation.check().map_err(cancelled)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(JOURNAL_PREFIX) || !name.ends_with(".json") {
            continue;
        }
        let path = entry.path();
        match fs::read(&path).map_err(io_error).and_then(|bytes| {
            serde_json::from_slice::<Journal>(&bytes).map_err(|error| {
                RewriteError::new("ast.rewrite.recovery_failed", error.to_string())
            })
        }) {
            Ok(journal) if !valid_journal(boundary, &journal) => {
                errors.push(format!("Invalid transaction journal: {}", path.display()));
            }
            Ok(journal) if journal.phase == "committed" => {
                errors.extend(finalize_committed(&path, &journal));
            }
            Ok(journal) => errors.extend(recover_one(&path, &journal)),
            Err(error) => errors.push(error.message),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(RewriteError::new(
            "ast.rewrite.recovery_failed",
            "An interrupted astRewrite transaction could not be recovered safely.",
        )
        .detail(json!({"errors":errors})))
    }
}

fn valid_journal(boundary: &Path, journal: &Journal) -> bool {
    journal.version == 1
        && journal.root == boundary
        && matches!(
            journal.phase.as_str(),
            "staging" | "prepared" | "committing" | "committed"
        )
        && journal.files.iter().enumerate().all(|(index, file)| {
            matches!(
                file.state.as_str(),
                "planned" | "staged" | "backed-up" | "promoted"
            ) && file.target.starts_with(boundary)
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
        Ok(bytes) => Ok(Some(sha256(bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn recover_one(path: &Path, journal: &Journal) -> Vec<String> {
    let mut errors = Vec::new();
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
                return Err(format!(
                    "Original bytes unavailable for recovery: {}",
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

fn finalize_committed(path: &Path, journal: &Journal) -> Vec<String> {
    let mut errors = Vec::new();
    for file in &journal.files {
        match hash_at(&file.target) {
            Ok(Some(hash)) if hash == file.after_hash => {}
            Ok(_) => {
                errors.push(format!(
                    "Committed target hash mismatch: {}",
                    file.target.display()
                ));
                continue;
            }
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

