//! Bounded, authenticated loose Git object reads. No pack fallback or Git process.
use crate::{filesystem::platform::EvidenceFile, NativeCancellation};
use flate2::{Decompress, FlushDecompress, Status};
use napi::{
    bindgen_prelude::{AsyncTask, Buffer},
    Env, Error, Result, Task,
};
use napi_derive::napi;
use sha1::{Digest, Sha1};
use std::{
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const CHUNK: usize = 64 * 1024;
const HEADER: usize = 64;

#[napi(object)]
pub struct GitObject {
    pub object_type: String,
    pub size: f64,
    pub content: Option<Buffer>,
}

pub struct GitObjectTask {
    path: String,
    oid: String,
    decoded_maximum: usize,
    compressed_maximum: usize,
    include_content: bool,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}

fn invalid(message: &str) -> String {
    format!("HISTORY_OBJECT_INVALID: {message}")
}

impl GitObjectTask {
    fn check(&self) -> std::result::Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("CANCELLED: Git object read cancelled".into());
        }
        if Instant::now() >= self.deadline {
            return Err("HISTORY_OBJECT_TIME_LIMIT: Git object read exceeded deadline".into());
        }
        Ok(())
    }

    fn read(&self) -> std::result::Result<(String, usize, Option<Vec<u8>>), String> {
        self.check()?;
        if self.oid.len() != 40
            || !self
                .oid
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("expected a lowercase SHA-1 object id"));
        }
        let mut source = EvidenceFile::open(&self.path, &self.cancelled).map_err(|e| {
            if e == "source_missing" {
                "HISTORY_OBJECT_UNAVAILABLE: Loose object missing; packed objects are unsupported"
                    .into()
            } else {
                e
            }
        })?;
        if source.size > self.compressed_maximum as u64 {
            return Err("HISTORY_OBJECT_LIMIT: compressed object exceeds read limit".into());
        }
        let mut input = [0u8; CHUNK];
        let mut output = [0u8; CHUNK];
        let mut decoder = Decompress::new(true);
        let mut hash = Sha1::new();
        let mut header = Vec::with_capacity(HEADER);
        let mut framing: Option<(String, usize)> = None;
        let mut body_size = 0usize;
        let mut content = self.include_content.then(Vec::new);
        let mut compressed = 0usize;
        loop {
            self.check()?;
            // Read at most one byte beyond the compressed limit to detect concurrent growth.
            let allowance = CHUNK.min(
                self.compressed_maximum
                    .saturating_sub(compressed)
                    .saturating_add(1),
            );
            let count = source
                .file
                .read(&mut input[..allowance])
                .map_err(|e| format!("IO_FAILURE: {e}"))?;
            compressed += count;
            if compressed > self.compressed_maximum {
                return Err(
                    "HISTORY_OBJECT_LIMIT: compressed object grew beyond read limit".into(),
                );
            }
            let mut consumed = 0usize;
            loop {
                self.check()?;
                let before_in = decoder.total_in();
                let before_out = decoder.total_out();
                let status = decoder
                    .decompress(&input[consumed..count], &mut output, FlushDecompress::None)
                    .map_err(|_| invalid("invalid zlib stream"))?;
                consumed += (decoder.total_in() - before_in) as usize;
                let produced = (decoder.total_out() - before_out) as usize;
                let bytes = &output[..produced];
                hash.update(bytes);
                let mut start = 0;
                if framing.is_none() {
                    for &byte in bytes {
                        start += 1;
                        if byte == 0 {
                            let text = std::str::from_utf8(&header)
                                .map_err(|_| invalid("non-ASCII header"))?;
                            let (kind, length) = text
                                .split_once(' ')
                                .ok_or_else(|| invalid("missing header size"))?;
                            if !matches!(kind, "blob" | "tree" | "commit" | "tag")
                                || length.is_empty()
                                || !length.bytes().all(|b| b.is_ascii_digit())
                                || (length.len() > 1 && length.starts_with('0'))
                            {
                                return Err(invalid("invalid type or canonical decimal size"));
                            }
                            let size = length.parse::<usize>().map_err(|_| {
                                "HISTORY_OBJECT_LIMIT: declared object size overflows".to_string()
                            })?;
                            if size > self.decoded_maximum {
                                return Err(
                                    "HISTORY_OBJECT_LIMIT: declared object size exceeds read limit"
                                        .into(),
                                );
                            }
                            if let Some(content) = &mut content {
                                content.try_reserve_exact(size).map_err(|_| "HISTORY_OBJECT_LIMIT: cannot allocate bounded object content".to_string())?;
                            }
                            framing = Some((kind.to_owned(), size));
                            break;
                        }
                        if header.len() == HEADER {
                            return Err(invalid("header exceeds 64 bytes"));
                        }
                        header.push(byte);
                    }
                }
                if let Some((_, size)) = &framing {
                    let body = &bytes[start..];
                    if body.len() > size.saturating_sub(body_size) {
                        return Err(invalid("object exceeds declared size"));
                    }
                    body_size += body.len();
                    if let Some(content) = &mut content {
                        content.extend_from_slice(body);
                    }
                }
                if status == Status::StreamEnd {
                    if consumed != count {
                        return Err(invalid("trailing compressed data"));
                    }
                    self.check()?;
                    if source
                        .file
                        .read(&mut input[..1])
                        .map_err(|e| format!("IO_FAILURE: {e}"))?
                        != 0
                    {
                        return Err(invalid("trailing compressed data"));
                    }
                    let (kind, size) = framing.ok_or_else(|| invalid("missing object header"))?;
                    if size != body_size {
                        return Err(invalid("truncated object body"));
                    }
                    if crate::digest_hex::lower_hex(hash.finalize()) != self.oid {
                        return Err("HISTORY_OBJECT_HASH_MISMATCH: decoded object does not match requested SHA-1".into());
                    }
                    source.recheck(&self.path, &self.cancelled)?;
                    self.check()?;
                    return Ok((kind, size, content));
                }
                if decoder.total_in() == before_in && produced == 0 {
                    if count == 0 || consumed != count {
                        return Err(invalid("truncated or stalled zlib stream"));
                    }
                    break;
                }
                if consumed == count && produced < CHUNK {
                    break;
                }
            }
            if count == 0 {
                return Err(invalid("truncated zlib stream"));
            }
        }
    }
}

impl Task for GitObjectTask {
    type Output = (String, usize, Option<Vec<u8>>);
    type JsValue = GitObject;
    fn compute(&mut self) -> Result<Self::Output> {
        self.read().map_err(Error::from_reason)
    }
    fn resolve(
        &mut self,
        _env: Env,
        (object_type, size, content): Self::Output,
    ) -> Result<GitObject> {
        Ok(GitObject {
            object_type,
            size: size as f64,
            content: content.map(Buffer::from),
        })
    }
}

#[napi]
pub fn read_git_object(
    path: String,
    oid: String,
    max_decoded_bytes: u32,
    max_compressed_bytes: u32,
    timeout_ms: u32,
    include_content: bool,
    cancellation: Option<&NativeCancellation>,
) -> AsyncTask<GitObjectTask> {
    AsyncTask::new(GitObjectTask {
        path,
        oid,
        decoded_maximum: max_decoded_bytes as usize,
        compressed_maximum: max_compressed_bytes as usize,
        include_content,
        deadline: Instant::now() + Duration::from_millis(timeout_ms as u64),
        cancelled: NativeCancellation::shared_flag(cancellation),
    })
}

#[napi(object)]
pub struct FileDurability {
    pub durable: bool,
    pub warnings: Vec<String>,
}
pub struct FlushFileTask {
    path: String,
    cancelled: Arc<AtomicBool>,
}

pub struct PrivateDirectoryTask {
    path: String,
    cancelled: Arc<AtomicBool>,
}
impl Task for PrivateDirectoryTask {
    type Output = ();
    type JsValue = ();
    fn compute(&mut self) -> Result<()> {
        crate::filesystem::platform::ensure_private_directory(&self.path, &self.cancelled)
            .map_err(Error::from_reason)
    }
    fn resolve(&mut self, _env: Env, _: ()) -> Result<()> {
        Ok(())
    }
}
#[napi]
pub fn ensure_private_directory(
    path: String,
    cancellation: Option<&NativeCancellation>,
) -> AsyncTask<PrivateDirectoryTask> {
    AsyncTask::new(PrivateDirectoryTask {
        path,
        cancelled: NativeCancellation::shared_flag(cancellation),
    })
}
impl Task for FlushFileTask {
    type Output = bool;
    type JsValue = FileDurability;
    fn compute(&mut self) -> Result<bool> {
        let source = EvidenceFile::open(&self.path, &self.cancelled).map_err(Error::from_reason)?;
        source
            .flush(&self.path, &self.cancelled)
            .map_err(Error::from_reason)
    }
    fn resolve(&mut self, _env: Env, durable: bool) -> Result<FileDurability> {
        Ok(FileDurability {
            durable,
            warnings: if durable {
                vec![]
            } else {
                vec![
                    "File data flushed; Windows cannot guarantee directory-entry persistence"
                        .into(),
                ]
            },
        })
    }
}
#[napi]
pub fn flush_file(
    path: String,
    cancellation: Option<&NativeCancellation>,
) -> AsyncTask<FlushFileTask> {
    AsyncTask::new(FlushFileTask {
        path,
        cancelled: NativeCancellation::shared_flag(cancellation),
    })
}
