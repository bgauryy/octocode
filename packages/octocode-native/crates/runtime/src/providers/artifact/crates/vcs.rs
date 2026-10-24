//! A published crate's packaging commit: `.cargo_vcs_info.json` read from
//! the `.crate` archive as it streams in.
use crate::providers::artifact::http::RegistryClient;
use crate::providers::artifact::util::{commit_sha, encode_component, parse_url, string};
use octocode_contract_types::ArtifactType;
use serde_json::Value;
use std::io::Write;
use std::ops::ControlFlow;

/// Decompressed bytes read while looking for one archive entry.
const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
/// Largest entry (the wanted file or a GNU long name) held in memory; a
/// `.cargo_vcs_info.json` is a few hundred bytes.
const MAX_ENTRY_BYTES: u64 = 64 * 1024;

/// The commit cargo recorded when it packaged `name@version`
/// (`.cargo_vcs_info.json` inside the `.crate` archive) and the crate's
/// directory inside its repository (`path_in_vcs`, empty at the root).
pub(super) async fn crate_vcs(
    name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Option<(String, Option<String>)> {
    // A published crate never changes: its VCS info (or its absence) is
    // read from the archive once.
    let resource = format!("crates:{name}@{version}:vcs");
    let info = match client.fact(&resource) {
        Some(info) => info,
        None => {
            let info = archive_vcs_info(name, version, client).await?;
            client.remember_fact(&resource, &info);
            info
        }
    };
    let sha = commit_sha(info.pointer("/git/sha1"))?;
    Some((sha, string(info.get("path_in_vcs"))))
}

/// `.cargo_vcs_info.json` of `name@version` (`{}` when the archive has
/// none); `None` when the archive cannot be read. The archive streams in
/// and the download stops at the entry: cargo packs it first, so a
/// multi-megabyte crate costs its first kilobytes.
async fn archive_vcs_info(name: &str, version: &str, client: &RegistryClient<'_>) -> Option<Value> {
    let url = parse_url(&format!(
        "https://static.crates.io/crates/{name}/{name}-{version}.crate",
        name = encode_component(name),
        version = encode_component(version),
    ))
    .ok()?;
    let mut reader = ArchiveEntry::new(&format!("{name}-{version}/.cargo_vcs_info.json"));
    client
        .stream(ArtifactType::Crates, url, &mut |chunk| reader.feed(chunk))
        .await
        .ok()
        .flatten()?;
    Some(
        reader
            .finish()
            .and_then(|info| serde_json::from_slice(&info).ok())
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
    )
}

/// Reads one entry of a gzip-compressed tar archive (POSIX ustar names with
/// prefixes, GNU long names) from compressed chunks as they arrive.
struct ArchiveEntry {
    gzip: flate2::write::GzDecoder<TarScan>,
}

impl ArchiveEntry {
    fn new(wanted: &str) -> Self {
        Self {
            gzip: flate2::write::GzDecoder::new(TarScan {
                wanted: wanted.to_owned(),
                pending: Vec::new(),
                state: TarState::Header,
                long_name: None,
                decompressed: 0,
                end: None,
            }),
        }
    }

    /// Decompress `chunk` into the scan; `Break` once the entry is read, the
    /// archive ended, or it cannot be read.
    fn feed(&mut self, chunk: &[u8]) -> ControlFlow<()> {
        let fed = self.gzip.write_all(chunk).and_then(|()| self.gzip.flush());
        let scan = self.gzip.get_mut();
        if fed.is_err() && scan.end.is_none() {
            scan.end = Some(ScanEnd::Missing);
        }
        if scan.end.is_some() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    /// The entry's bytes, `None` when the archive lacks it or is unreadable.
    fn finish(mut self) -> Option<Vec<u8>> {
        if self.gzip.get_ref().end.is_none() {
            let _ = self.gzip.try_finish();
        }
        match self.gzip.get_mut().end.take() {
            Some(ScanEnd::Found(body)) => Some(body),
            Some(ScanEnd::Missing) | None => None,
        }
    }
}

/// Where the tar scan is: before a header, inside an entry it skips
/// (`remaining` bytes with padding), or inside one it holds (a GNU long name
/// or the wanted file, `size` bytes padded to `padded`).
enum TarState {
    Header,
    Skip { remaining: u64 },
    Hold { size: u64, padded: u64, kind: u8 },
}

/// A tar reader fed decompressed bytes: it buffers at most one header or
/// one held entry, skipping every other entry as it passes.
struct TarScan {
    wanted: String,
    pending: Vec<u8>,
    state: TarState,
    long_name: Option<String>,
    decompressed: u64,
    /// Set once the scan ends.
    end: Option<ScanEnd>,
}

/// How a finished tar scan ended.
enum ScanEnd {
    /// The wanted entry's bytes.
    Found(Vec<u8>),
    /// The entry is absent or oversized, or the archive is malformed.
    Missing,
}

impl std::io::Write for TarScan {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.end.is_none() {
            self.decompressed = self.decompressed.saturating_add(bytes.len() as u64);
            if self.decompressed > MAX_ARCHIVE_BYTES {
                self.end = Some(ScanEnd::Missing);
            } else {
                self.pending.extend_from_slice(bytes);
                self.advance();
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl TarScan {
    fn advance(&mut self) {
        while self.end.is_none() {
            match self.state {
                TarState::Header => {
                    let Some(header) = self.pending.get(..512) else {
                        return;
                    };
                    let header: [u8; 512] = header.try_into().unwrap_or([0; 512]);
                    self.pending.drain(..512);
                    self.state = self.header(&header);
                }
                TarState::Skip { remaining } => {
                    let skipped = remaining.min(self.pending.len() as u64);
                    self.pending.drain(..skipped as usize);
                    if skipped < remaining {
                        self.state = TarState::Skip {
                            remaining: remaining - skipped,
                        };
                        return;
                    }
                    self.state = TarState::Header;
                }
                TarState::Hold { size, padded, kind } => {
                    if (self.pending.len() as u64) < padded {
                        return;
                    }
                    let body = self.pending[..size as usize].to_vec();
                    self.pending.drain(..padded as usize);
                    self.state = TarState::Header;
                    if kind == b'L' {
                        let end = body
                            .iter()
                            .position(|byte| *byte == 0)
                            .unwrap_or(body.len());
                        self.long_name = Some(String::from_utf8_lossy(&body[..end]).into_owned());
                    } else {
                        self.end = Some(ScanEnd::Found(body));
                    }
                }
            }
        }
    }

    /// The state after `header`; an end-of-archive block, a malformed
    /// header or an oversized held entry ends the scan.
    fn header(&mut self, header: &[u8; 512]) -> TarState {
        if header.iter().all(|byte| *byte == 0) {
            self.end = Some(ScanEnd::Missing);
            return TarState::Header;
        }
        let field = |range: std::ops::Range<usize>| {
            let bytes = &header[range];
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len());
            String::from_utf8_lossy(&bytes[..end]).into_owned()
        };
        let Ok(size) = u64::from_str_radix(field(124..136).trim(), 8) else {
            self.end = Some(ScanEnd::Missing);
            return TarState::Header;
        };
        let kind = header[156];
        let name = self.long_name.take().unwrap_or_else(|| {
            let prefix = field(345..500);
            let name = field(0..100);
            if &header[257..263] == b"ustar\0" && !prefix.is_empty() {
                format!("{prefix}/{name}")
            } else {
                name
            }
        });
        let padded = size.div_ceil(512) * 512;
        let held = kind == b'L' || (name == self.wanted && matches!(kind, b'0' | 0));
        if !held {
            return TarState::Skip { remaining: padded };
        }
        if size > MAX_ENTRY_BYTES {
            self.end = Some(ScanEnd::Missing);
            return TarState::Header;
        }
        TarState::Hold { size, padded, kind }
    }
}

#[cfg(test)]
fn test_header(name: &str, size: usize, kind: u8) -> [u8; 512] {
    let mut header = [0u8; 512];
    let bytes = name.as_bytes();
    header[..bytes.len().min(100)].copy_from_slice(&bytes[..bytes.len().min(100)]);
    header[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
    header[156] = kind;
    header[257..262].copy_from_slice(b"ustar");
    header
}

/// A gzip-compressed ustar archive of `(name, body, typeflag)` entries.
#[cfg(test)]
pub(super) fn test_archive(entries: &[(&str, &[u8], u8)]) -> Vec<u8> {
    let mut tar = Vec::new();
    for (name, body, kind) in entries {
        tar.extend_from_slice(&test_header(name, body.len(), *kind));
        tar.extend_from_slice(body);
        tar.resize(tar.len().div_ceil(512) * 512, 0);
    }
    tar.extend_from_slice(&[0u8; 1024]);
    use std::io::Write;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&tar).expect("gzip");
    gz.finish().expect("gzip")
}

#[cfg(test)]
mod tests;
