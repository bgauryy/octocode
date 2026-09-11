mod diff;
pub mod evidence;
pub mod filesystem;
pub mod git_object;

use napi::bindgen_prelude::{AsyncTask, Buffer};
use napi::{Env, Error, Result, Task};
use napi_derive::napi;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[napi]
pub struct NativeCancellation {
    flag: Arc<AtomicBool>,
}

#[napi]
impl NativeCancellation {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }
    #[napi]
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }

    pub(crate) fn shared_flag(cancellation: Option<&Self>) -> Arc<AtomicBool> {
        cancellation
            .map(|value| Arc::clone(&value.flag))
            .unwrap_or_default()
    }
}
impl Default for NativeCancellation {
    fn default() -> Self {
        Self::new()
    }
}

#[napi(object)]
pub struct FileSnapshot {
    pub exists: bool,
    pub kind: String,
    pub version: String,
    pub digest: Option<String>,
    pub content: Option<Buffer>,
    pub mode: Option<u32>,
    pub size: f64,
}

#[napi(object)]
pub struct MutationReceipt {
    pub committed: bool,
    pub durable: bool,
    pub warnings: Vec<String>,
    pub bytes: f64,
}

pub struct SnapshotTask {
    path: String,
    maximum: usize,
    content: bool,
    symlink: bool,
    cancelled: Arc<AtomicBool>,
}
impl Task for SnapshotTask {
    type Output = filesystem::Snapshot;
    type JsValue = FileSnapshot;
    fn compute(&mut self) -> Result<Self::Output> {
        filesystem::snapshot_cancellable(
            &self.path,
            self.maximum,
            self.content,
            self.symlink,
            &self.cancelled,
        )
        .map_err(Error::from_reason)
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(FileSnapshot {
            exists: output.exists,
            kind: output.kind,
            version: output.version,
            digest: output.digest,
            content: output.content.map(Buffer::from),
            mode: output.mode,
            size: output.size as f64,
        })
    }
}

#[napi]
pub fn snapshot_file(
    path: String,
    max_bytes: u32,
    include_content: bool,
    allow_leaf_symlink: Option<bool>,
    cancellation: Option<&NativeCancellation>,
) -> AsyncTask<SnapshotTask> {
    AsyncTask::new(SnapshotTask {
        path,
        maximum: max_bytes as usize,
        content: include_content,
        symlink: allow_leaf_symlink.unwrap_or(false),
        cancelled: NativeCancellation::shared_flag(cancellation),
    })
}

pub struct MutationTask {
    path: String,
    bytes: Option<Vec<u8>>,
    expected: String,
    maximum: usize,
    mode: Option<u32>,
    parent_mode: Option<u32>,
    cancelled: Arc<AtomicBool>,
}
impl Task for MutationTask {
    type Output = filesystem::Receipt;
    type JsValue = MutationReceipt;
    fn compute(&mut self) -> Result<Self::Output> {
        match &self.bytes {
            Some(bytes) => filesystem::replace(
                &self.path,
                bytes,
                &self.expected,
                self.maximum,
                self.mode,
                &self.cancelled,
                self.parent_mode,
            ),
            None => filesystem::delete(&self.path, &self.expected, self.maximum, &self.cancelled),
        }
        .map_err(Error::from_reason)
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(MutationReceipt {
            committed: output.committed,
            durable: output.durable,
            warnings: output.warnings,
            bytes: output.bytes as f64,
        })
    }
}

#[napi]
pub fn replace_file(
    path: String,
    content: Buffer,
    expected_version: String,
    max_bytes: u32,
    create_mode: Option<u32>,
    cancellation: Option<&NativeCancellation>,
    parent_mode: Option<u32>,
) -> AsyncTask<MutationTask> {
    AsyncTask::new(MutationTask {
        path,
        bytes: Some(content.to_vec()),
        expected: expected_version,
        maximum: max_bytes as usize,
        mode: create_mode,
        parent_mode,
        cancelled: NativeCancellation::shared_flag(cancellation),
    })
}

#[napi]
pub fn delete_file(
    path: String,
    expected_version: String,
    max_bytes: u32,
    cancellation: Option<&NativeCancellation>,
) -> AsyncTask<MutationTask> {
    AsyncTask::new(MutationTask {
        path,
        bytes: None,
        expected: expected_version,
        maximum: max_bytes as usize,
        mode: None,
        parent_mode: None,
        cancelled: NativeCancellation::shared_flag(cancellation),
    })
}

#[napi(object)]
pub struct DiffOperation {
    pub op_type: String,
    pub line: String,
}

#[napi]
pub fn compute_line_diff(old_text: String, new_text: String) -> Vec<DiffOperation> {
    diff::line_diff(&old_text, &new_text)
}

pub struct DiffTask {
    old: String,
    new: String,
}
impl Task for DiffTask {
    type Output = Vec<DiffOperation>;
    type JsValue = Vec<DiffOperation>;
    fn compute(&mut self) -> Result<Self::Output> {
        Ok(diff::line_diff(&self.old, &self.new))
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

#[napi]
pub fn compute_line_diff_async(old_text: String, new_text: String) -> AsyncTask<DiffTask> {
    AsyncTask::new(DiffTask {
        old: old_text,
        new: new_text,
    })
}

#[napi(object)]
pub struct DiffArtifacts {
    pub diff: String,
    pub patch: String,
}

pub struct DiffArtifactsTask {
    path: String,
    old: String,
    new: String,
}
impl Task for DiffArtifactsTask {
    type Output = DiffArtifacts;
    type JsValue = DiffArtifacts;
    fn compute(&mut self) -> Result<Self::Output> {
        Ok(diff::diff_artifacts(&self.path, &self.old, &self.new))
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

#[napi]
pub fn generate_diff_artifacts(
    file_path: String,
    old_text: String,
    new_text: String,
) -> DiffArtifacts {
    diff::diff_artifacts(&file_path, &old_text, &new_text)
}

#[napi]
pub fn generate_diff_artifacts_async(
    file_path: String,
    old_text: String,
    new_text: String,
) -> AsyncTask<DiffArtifactsTask> {
    AsyncTask::new(DiffArtifactsTask {
        path: file_path,
        old: old_text,
        new: new_text,
    })
}
