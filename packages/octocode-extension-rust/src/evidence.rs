//! Awareness v1 evidence: one bounded streaming capture and metadata-only final recheck.
use crate::{filesystem::platform::EvidenceFile, NativeCancellation};
use napi::{bindgen_prelude::AsyncTask, Env, Result, Task};
use napi_derive::napi;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Component, Path};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

const PREFIX: &str = "awareness-evidence-v1:";

#[napi(object)]
pub struct FileFingerprint {
    pub fingerprint: Option<String>,
    pub reason: Option<String>,
    pub paths: Vec<String>,
    pub files: u32,
    pub bytes: f64,
}

pub struct FingerprintTask {
    root: String,
    paths: Vec<String>,
    file_maximum: u64,
    batch_maximum: u64,
    files_maximum: usize,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}

fn reason(error: String) -> String {
    if error.starts_with("CANCELLED:") {
        "cancelled".into()
    } else if error.starts_with("UNSAFE_PATH:") {
        "symlink_source".into()
    } else if error.starts_with("PRECONDITION_FAILED:") {
        "source_changed_during_read".into()
    } else if error.starts_with("INVALID_PATH:") {
        "foreign_source".into()
    } else if error.starts_with("NOT_REGULAR_FILE:") {
        "not_regular_file".into()
    } else if error.starts_with("IO_FAILURE:") {
        "source_inaccessible".into()
    } else {
        error
    }
}

impl FingerprintTask {
    fn check(&self) -> std::result::Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if Instant::now() >= self.deadline {
            return Err("time_limit".into());
        }
        Ok(())
    }

    fn capture(&self, output: &mut FileFingerprint) -> std::result::Result<String, String> {
        self.check()?;
        if self.paths.is_empty() {
            return Err("no_file_references".into());
        }
        if self.paths.len() > self.files_maximum {
            return Err("reference_limit".into());
        }
        let root = Path::new(&self.root);
        if !root.is_absolute()
            || root
                .components()
                .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
        {
            return Err("foreign_source".into());
        }
        let mut paths = self.paths.clone();
        // JavaScript Array.sort orders UTF-16 code units, not Unicode scalar values.
        paths.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        paths.dedup();
        for path in &paths {
            let parsed = Path::new(path);
            if parsed
                .components()
                .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
                || parsed
                    .strip_prefix(root)
                    .map_or(true, |p| p.as_os_str().is_empty())
            {
                return Err("foreign_source".into());
            }
        }
        output.paths = paths;
        let mut hasher = Sha256::new();
        hasher.update(serde_json::to_vec(&(PREFIX, &self.root)).expect("string JSON"));
        let mut observed = Vec::new();
        let mut total = 0u64;
        for path in &output.paths {
            self.check()?;
            output.files += 1;
            let mut file = EvidenceFile::open(path, &self.cancelled).map_err(reason)?;
            self.check()?;
            if file.size > self.file_maximum {
                return Err("source_too_large".into());
            }
            if total + file.size > self.batch_maximum {
                return Err("byte_limit".into());
            }
            let relative = Path::new(path)
                .strip_prefix(root)
                .expect("contained path")
                .to_str()
                .expect("Unicode path");
            hasher.update(
                serde_json::to_vec(&(relative, file.mode, file.size)).expect("metadata JSON"),
            );
            let mut buffer = [0u8; 64 * 1024];
            let mut count = 0u64;
            loop {
                self.check()?;
                // One sentinel byte detects growth; memory stays constant even for huge caller limits.
                let limit = (file.size - count + 1).min(buffer.len() as u64) as usize;
                let length = file
                    .file
                    .read(&mut buffer[..limit])
                    .map_err(|_| "source_inaccessible")?;
                self.check()?;
                if length == 0 {
                    break;
                }
                count += length as u64;
                total += length as u64;
                output.bytes = total as f64;
                if count > file.size {
                    return Err("source_changed_during_read".into());
                }
                hasher.update(&buffer[..length]);
            }
            if count != file.size {
                return Err("source_changed_during_read".into());
            }
            file.recheck(path, &self.cancelled).map_err(reason)?;
            observed.push(file);
        }
        for (path, file) in output.paths.iter().zip(&observed) {
            self.check()?;
            file.recheck(path, &self.cancelled).map_err(reason)?;
        }
        self.check()?;
        Ok(format!("{PREFIX}{:x}", hasher.finalize()))
    }
}

impl Task for FingerprintTask {
    type Output = FileFingerprint;
    type JsValue = FileFingerprint;
    fn compute(&mut self) -> Result<Self::Output> {
        Ok(self.run())
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

impl FingerprintTask {
    fn run(&self) -> FileFingerprint {
        let mut output = FileFingerprint {
            fingerprint: None,
            reason: None,
            paths: Vec::new(),
            files: 0,
            bytes: 0.0,
        };
        match self.capture(&mut output) {
            Ok(value) => output.fingerprint = Some(value),
            Err(value) => output.reason = Some(value),
        }
        output
    }
}

#[napi]
pub fn fingerprint_files(
    root: String,
    paths: Vec<String>,
    max_file_bytes: u32,
    max_batch_bytes: u32,
    max_files: u32,
    timeout_ms: u32,
    cancellation: Option<&NativeCancellation>,
) -> AsyncTask<FingerprintTask> {
    AsyncTask::new(FingerprintTask {
        root,
        paths,
        file_maximum: max_file_bytes as u64,
        batch_maximum: max_batch_bytes as u64,
        files_maximum: max_files as usize,
        deadline: Instant::now() + Duration::from_millis(timeout_ms as u64),
        cancelled: NativeCancellation::shared_flag(cancellation),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn task(root: &str, paths: Vec<String>) -> FingerprintTask {
        FingerprintTask {
            root: root.into(),
            paths,
            file_maximum: 10,
            batch_maximum: 10,
            files_maximum: 4,
            deadline: Instant::now() + Duration::from_secs(10),
            cancelled: Arc::default(),
        }
    }

    #[test]
    fn exact_empty_file_composition_and_counters() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let path = root.join("empty");
        fs::write(&path, []).unwrap();
        let request = task(root.to_str().unwrap(), vec![path.to_str().unwrap().into()]);
        let result = request.run();
        let file = EvidenceFile::open(path.to_str().unwrap(), &AtomicBool::new(false)).unwrap();
        let mut hash = Sha256::new();
        hash.update(format!(
            "[\"awareness-evidence-v1:\",{}][\"empty\",{},0]",
            serde_json::to_string(root.to_str().unwrap()).unwrap(),
            file.mode
        ));
        assert_eq!(
            result.fingerprint,
            Some(format!("{PREFIX}{:x}", hash.finalize()))
        );
        assert_eq!(result.files, 1);
        assert_eq!(result.bytes, 0.0);
    }

    #[test]
    fn aggregate_byte_bound_includes_completed_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let paths: Vec<_> = ["a", "b"]
            .iter()
            .map(|name| {
                let p = root.join(name);
                fs::write(&p, b"123456").unwrap();
                p.to_str().unwrap().to_owned()
            })
            .collect();
        let result = task(root.to_str().unwrap(), paths).run();
        assert_eq!(result.reason.as_deref(), Some("byte_limit"));
        assert!(result.fingerprint.is_none());
        assert_eq!(result.bytes, 6.0);
        assert_eq!(result.files, 2);
    }

    #[test]
    fn expired_enqueued_request_does_not_open_files() {
        let mut request = task("/", vec!["/missing".into()]);
        request.deadline = Instant::now();
        let result = request.run();
        assert_eq!(result.reason.as_deref(), Some("time_limit"));
        assert_eq!(result.files, 0);
    }

    #[test]
    fn evidence_recheck_rejects_growth_replacement_and_ancestor_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let parent = root.join("parent");
        fs::create_dir(&parent).unwrap();
        let path = parent.join("a");
        fs::write(&path, b"1234").unwrap();
        let cancel = AtomicBool::new(false);
        let captured = EvidenceFile::open(path.to_str().unwrap(), &cancel).unwrap();
        fs::write(&path, b"12345").unwrap();
        assert!(captured.recheck(path.to_str().unwrap(), &cancel).is_err());
        drop(captured);
        let captured = EvidenceFile::open(path.to_str().unwrap(), &cancel).unwrap();
        fs::rename(&path, parent.join("old")).unwrap();
        fs::write(&path, b"12345").unwrap();
        assert!(captured.recheck(path.to_str().unwrap(), &cancel).is_err());
        drop(captured);
        let captured = EvidenceFile::open(path.to_str().unwrap(), &cancel).unwrap();
        fs::rename(&parent, root.join("old-parent")).unwrap();
        fs::create_dir(&parent).unwrap();
        fs::write(&path, b"12345").unwrap();
        assert!(captured.recheck(path.to_str().unwrap(), &cancel).is_err());
    }
}
