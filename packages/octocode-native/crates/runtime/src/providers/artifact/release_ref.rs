//! Release refs for registries whose metadata names no commit. Each is a
//! lead source only: any failure leaves the lookup without a release ref.
use super::http::RegistryClient;
use super::util::{commit_sha, encode_component, github_repo_dir, github_slug, parse_url, string};
use super::{ArtifactItem, ArtifactType};
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
pub(crate) async fn crate_vcs(
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
                found: None,
            }),
        }
    }

    /// Decompress `chunk` into the scan; `Break` once the entry is read, the
    /// archive ended, or it cannot be read.
    fn feed(&mut self, chunk: &[u8]) -> ControlFlow<()> {
        let fed = self.gzip.write_all(chunk).and_then(|()| self.gzip.flush());
        let scan = self.gzip.get_mut();
        if fed.is_err() && scan.found.is_none() {
            scan.found = Some(None);
        }
        if scan.found.is_some() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    /// The entry's bytes, `None` when the archive lacks it or is unreadable.
    fn finish(mut self) -> Option<Vec<u8>> {
        if self.gzip.get_ref().found.is_none() {
            let _ = self.gzip.try_finish();
        }
        self.gzip.get_mut().found.take().flatten()
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
    /// Set once the scan ends: the entry, or `None` when it is absent,
    /// oversized, or the archive is malformed.
    found: Option<Option<Vec<u8>>>,
}

impl std::io::Write for TarScan {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.found.is_none() {
            self.decompressed = self.decompressed.saturating_add(bytes.len() as u64);
            if self.decompressed > MAX_ARCHIVE_BYTES {
                self.found = Some(None);
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
        while self.found.is_none() {
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
                        self.found = Some(Some(body));
                    }
                }
            }
        }
    }

    /// The state after `header`; an end-of-archive block, a malformed
    /// header or an oversized held entry ends the scan.
    fn header(&mut self, header: &[u8; 512]) -> TarState {
        if header.iter().all(|byte| *byte == 0) {
            self.found = Some(None);
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
            self.found = Some(None);
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
            self.found = Some(None);
            return TarState::Header;
        }
        TarState::Hold { size, padded, kind }
    }
}

/// A pending release-tag check.
pub type TagFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = Option<bool>> + 'a>>;

/// Checks that a release tag or a recorded commit exists in a GitHub
/// repository, through the configured GitHub API (its URL, the caller's
/// credential and request budget). Each answers `Some(true)` when it
/// exists, `Some(false)` when the repository has none, `None` when the
/// check failed.
pub trait ReleaseTags: Send + Sync {
    /// `tag` is a tag of `owner/repo` (a branch of that name is not).
    fn tag_exists<'a>(&'a self, owner: &'a str, repo: &'a str, tag: &'a str) -> TagFuture<'a>;
    /// `commit` (a SHA a registry recorded) is a commit of `owner/repo`.
    fn commit_exists<'a>(&'a self, owner: &'a str, repo: &'a str, commit: &'a str)
    -> TagFuture<'a>;
}

/// Pins `artifact`'s source to the upstream release tag of `version` when
/// its registry named no commit: the one fallback every registry without a
/// recorded release commit (or whose commit is unknown) takes. `package` is
/// the name the package's tags go by (Maven's artifactId, not its
/// coordinate).
pub(crate) async fn pin_release_tag(
    artifact: &mut ArtifactItem,
    package: &str,
    version: &str,
    client: &RegistryClient<'_>,
) {
    if artifact.source_ref.is_some() {
        return;
    }
    let Some(repository) = artifact.repository.as_deref() else {
        return;
    };
    let directory = artifact
        .repository_directory
        .clone()
        .or_else(|| github_repo_dir(repository));
    artifact.source_ref =
        github_release_tag(repository, directory.as_deref(), package, version, client).await;
    artifact.source_tag = artifact.source_ref.is_some();
}

/// The release tag of `version` in a GitHub repository, when one exists
/// upstream: `v<version>`, `<version>`, then the shapes a monorepo uses for
/// one package's releases ([`tag_candidates`]). Without a GitHub check, or
/// when the check fails (rate limit, missing repository), there is no tag.
/// A found tag is remembered as a release fact, so a repeated lookup spends
/// no GitHub request; an absence is remembered as long as a registry answer
/// (the tag may be pushed later).
async fn github_release_tag(
    repository: &str,
    directory: Option<&str>,
    package: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Option<String> {
    let tags = client.tags?;
    let (owner, repo) = github_slug(repository)?;
    let resource = format!(
        "tag:{}/{}/{}:{package}@{version}",
        owner.to_ascii_lowercase(),
        repo.to_ascii_lowercase(),
        directory.unwrap_or_default()
    );
    if let Some(fact) = client.fact(&resource) {
        return string(fact.get("tag"));
    }
    let candidates = tag_candidates(&repo, package, directory, version);
    // Two rounds: the plain tags most releases carry, then a monorepo
    // package's own shapes, each round's checks together.
    let (plain, own) = candidates.split_at(candidates.len().min(2));
    for round in [plain, own] {
        let answers = futures_util::future::join_all(
            round.iter().map(|tag| tags.tag_exists(&owner, &repo, tag)),
        )
        .await;
        for (tag, exists) in round.iter().zip(answers) {
            // A failed check is no answer: nothing is remembered.
            if exists? {
                client.remember_fact(&resource, &serde_json::json!({ "tag": tag }));
                return Some(tag.clone());
            }
        }
    }
    client.remember_answer(&resource, &serde_json::json!({ "tag": null }));
    None
}

/// Tags a package's release may carry, most common first: `v<version>` and
/// `<version>`, then `<stem>-v<version>` and `<stem>@<version>` for each name
/// the package goes by inside its repository: the package name (scoped or
/// not), its directory, and either without the repository name's prefix
/// (`pydantic_core` in `pydantic` is tagged `core-v2.50.1`). A package that
/// is its repository, with no directory of its own, is tagged without a stem.
fn tag_candidates(
    repo: &str,
    package: &str,
    directory: Option<&str>,
    version: &str,
) -> Vec<String> {
    let mut tags = vec![format!("v{version}"), version.to_owned()];
    for stem in tag_stems(repo, package, directory) {
        for tag in [format!("{stem}-v{version}"), format!("{stem}@{version}")] {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }
    tags
}

fn tag_stems(repo: &str, package: &str, directory: Option<&str>) -> Vec<String> {
    let key = |name: &str| name.to_ascii_lowercase().replace('_', "-");
    let repo_key = key(repo);
    let unscoped = package.rsplit('/').next().unwrap_or(package);
    let directory = directory
        .and_then(|directory| directory.rsplit('/').next())
        .filter(|leaf| !leaf.is_empty());
    if directory.is_none() && key(unscoped) == repo_key {
        return Vec::new();
    }
    let without_repo = |name: &str| {
        key(name)
            .strip_prefix(&repo_key)
            .and_then(|rest| rest.strip_prefix('-'))
            .filter(|rest| !rest.is_empty())
            .map(str::to_owned)
    };
    let mut stems: Vec<String> = Vec::new();
    for stem in [
        Some(package.to_owned()),
        Some(unscoped.to_owned()),
        directory.map(str::to_owned),
        Some(key(unscoped)),
        without_repo(unscoped),
        directory.and_then(without_repo),
    ]
    .into_iter()
    .flatten()
    {
        if !stem.is_empty() && !stems.contains(&stem) {
            stems.push(stem);
        }
    }
    stems
}

/// Whether GitHub says `repository` has no commit `reference`: `false` when
/// it has one, when the repository is not on GitHub, or when no check could
/// run (no GitHub access, rate limit), so only a definite answer drops a ref.
pub(crate) async fn github_ref_missing(
    repository: &str,
    reference: &str,
    client: &RegistryClient<'_>,
) -> bool {
    let (Some(tags), Some((owner, repo))) = (client.tags, github_slug(repository)) else {
        return false;
    };
    tags.commit_exists(&owner, &repo, reference).await == Some(false)
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
pub(crate) fn test_archive(entries: &[(&str, &[u8], u8)]) -> Vec<u8> {
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
mod tests {
    use super::*;

    /// One file's bytes from a whole gzip-compressed tar archive, fed to
    /// the streaming reader as one chunk.
    fn archive_entry(archive: &[u8], wanted: &str) -> Option<Vec<u8>> {
        let mut reader = ArchiveEntry::new(wanted);
        let _ = reader.feed(archive);
        reader.finish()
    }

    #[test]
    fn a_monorepo_package_is_tagged_by_the_names_it_goes_by() {
        // pydantic-core is released from pydantic/pydantic as `core-v<version>`.
        let tags = tag_candidates("pydantic", "pydantic_core", Some("pydantic-core"), "2.50.1");
        assert_eq!(&tags[..2], ["v2.50.1", "2.50.1"]);
        assert!(tags.contains(&"core-v2.50.1".to_owned()), "{tags:?}");
        assert!(
            tags.contains(&"pydantic-core-v2.50.1".to_owned()),
            "{tags:?}"
        );
        // A scoped package keeps its scope in the `@` form changesets writes.
        let tags = tag_candidates("changesets", "@changesets/cli", None, "2.29.0");
        assert!(
            tags.contains(&"@changesets/cli@2.29.0".to_owned()),
            "{tags:?}"
        );
        assert!(tags.contains(&"cli@2.29.0".to_owned()), "{tags:?}");
        // A package that is its repository takes only the plain tags.
        assert_eq!(
            tag_candidates("requests", "requests", None, "2.32.3"),
            ["v2.32.3", "2.32.3"]
        );
        assert_eq!(
            tag_candidates("serde_json", "serde-json", None, "1.0.1"),
            ["v1.0.1", "1.0.1"]
        );
    }

    #[test]
    fn reads_the_vcs_info_entry_wherever_it_sits() {
        let info =
            br#"{"git":{"sha1":"EA6D652A102DEE3F22B490DB70545B7F66A23FB7"},"path_in_vcs":"tokio"}"#;
        let first = test_archive(&[
            ("tokio-1.40.0/.cargo_vcs_info.json", info, b'0'),
            ("tokio-1.40.0/Cargo.toml", b"[package]", b'0'),
        ]);
        let last = test_archive(&[
            ("tokio-1.40.0/Cargo.toml", &[b'x'; 700], b'0'),
            ("tokio-1.40.0/.cargo_vcs_info.json", info, b'0'),
        ]);
        for bytes in [first, last] {
            assert_eq!(
                archive_entry(&bytes, "tokio-1.40.0/.cargo_vcs_info.json").as_deref(),
                Some(&info[..])
            );
        }
        // A GNU long name names the entry that follows it.
        let long = "x".repeat(120);
        let wanted = format!("{long}/.cargo_vcs_info.json");
        let bytes = test_archive(&[
            ("././@LongLink", format!("{wanted}\0").as_bytes(), b'L'),
            ("truncated", info, b'0'),
        ]);
        assert_eq!(archive_entry(&bytes, &wanted).as_deref(), Some(&info[..]));
        assert_eq!(archive_entry(&bytes, "absent"), None);
        assert_eq!(archive_entry(b"not gzip", "absent"), None);
    }

    #[test]
    fn an_oversized_wanted_or_long_name_entry_is_never_buffered() {
        let big = vec![b'x'; MAX_ENTRY_BYTES as usize + 1];
        let bytes = test_archive(&[("c-1/.cargo_vcs_info.json", &big, b'0')]);
        assert_eq!(archive_entry(&bytes, "c-1/.cargo_vcs_info.json"), None);
        let bytes = test_archive(&[("././@LongLink", &big, b'L'), ("x", b"{}", b'0')]);
        assert_eq!(archive_entry(&bytes, "x"), None);
    }

    fn tag_client<'a>(
        http: &'a super::super::test_http::RouteHttp,
        budget: &'a octocode_io::budget::RequestBudget,
        cache: &'a super::super::ArtifactCache,
        tags: &'a super::super::test_http::KnownTags,
    ) -> RegistryClient<'a> {
        RegistryClient {
            http,
            budget,
            cache: Some(cache),
            tags: Some(tags),
        }
    }

    /// A package with no matching tag is checked once per registry-answer
    /// lifetime: the repeat lookup spends no GitHub request.
    #[tokio::test]
    async fn an_absent_release_tag_is_remembered_for_a_registry_answer_lifetime() {
        let http = super::super::test_http::RouteHttp::new();
        let budget = super::super::types::test_budget();
        let cache = super::super::ArtifactCache::new(None);
        let tags = super::super::test_http::KnownTags::always(Some(false));
        let client = tag_client(&http, &budget, &cache, &tags);
        let repository = "https://github.com/acme/monorepo";
        for _ in 0..2 {
            assert_eq!(
                github_release_tag(
                    repository,
                    Some("packages/widget"),
                    "widget",
                    "1.0.0",
                    &client
                )
                .await,
                None
            );
        }
        let candidates = tag_candidates("monorepo", "widget", Some("packages/widget"), "1.0.0");
        assert_eq!(tags.seen().len(), candidates.len(), "{:?}", tags.seen());
        // A failed check is not an absence: it is asked again.
        let failing = super::super::test_http::KnownTags::always(None);
        let client = tag_client(&http, &budget, &cache, &failing);
        for _ in 0..2 {
            assert_eq!(
                github_release_tag(repository, None, "other", "2.0.0", &client).await,
                None
            );
        }
        assert_eq!(failing.seen().len(), 4, "{:?}", failing.seen());
    }

    /// The plain tags are asked first; a monorepo package's own tags are
    /// asked together after them, and the earliest candidate that exists
    /// wins whatever order the answers arrive in.
    #[tokio::test]
    async fn release_tags_are_asked_in_two_rounds_and_the_first_candidate_wins() {
        let http = super::super::test_http::RouteHttp::new();
        let budget = super::super::types::test_budget();
        let cache = super::super::ArtifactCache::new(None);
        let plain = super::super::test_http::KnownTags::new(vec!["acme/widget@v1.0.0"]);
        let client = tag_client(&http, &budget, &cache, &plain);
        assert_eq!(
            github_release_tag(
                "https://github.com/acme/widget",
                Some("packages/core"),
                "core",
                "1.0.0",
                &client
            )
            .await
            .as_deref(),
            Some("v1.0.0")
        );
        assert_eq!(plain.seen(), ["acme/widget@v1.0.0", "acme/widget@1.0.0"]);
        let stems = super::super::test_http::KnownTags::new(vec![
            "acme/widget@core@1.0.0",
            "acme/widget@core-v1.0.0",
        ]);
        let cache = super::super::ArtifactCache::new(None);
        let client = tag_client(&http, &budget, &cache, &stems);
        assert_eq!(
            github_release_tag(
                "https://github.com/acme/widget",
                Some("packages/core"),
                "core",
                "1.0.0",
                &client
            )
            .await
            .as_deref(),
            Some("core-v1.0.0")
        );
        assert_eq!(
            stems.seen().len(),
            tag_candidates("widget", "core", Some("packages/core"), "1.0.0").len()
        );
    }

    /// A published crate's VCS info is read from its archive once.
    #[tokio::test]
    async fn crate_vcs_info_is_downloaded_once() {
        let name = "vcs-once";
        let info =
            br#"{"git":{"sha1":"b6a77c4413f902523646be0d7f5520631df53ff6"},"path_in_vcs":"x"}"#;
        let entry = format!("{name}-1.0.0/.cargo_vcs_info.json");
        let http = super::super::test_http::SequenceHttp::new(vec![test_archive(&[(
            entry.as_str(),
            &info[..],
            b'0',
        )])]);
        let budget = super::super::types::test_budget();
        let cache = super::super::ArtifactCache::new(None);
        let client = RegistryClient {
            http: &http,
            budget: &budget,
            cache: Some(&cache),
            tags: None,
        };
        let expected = Some((
            "b6a77c4413f902523646be0d7f5520631df53ff6".to_owned(),
            Some("x".to_owned()),
        ));
        assert_eq!(crate_vcs(name, "1.0.0", &client).await, expected);
        assert_eq!(crate_vcs(name, "1.0.0", &client).await, expected);
        assert_eq!(http.calls(), 1);
    }

    /// The archive streams in and the download stops at the VCS entry:
    /// cargo packs it first, so a large crate is read for its first pieces
    /// only; an entry packed last is still found.
    #[tokio::test]
    async fn crate_vcs_info_stops_the_download_at_its_entry() {
        let info =
            br#"{"git":{"sha1":"b6a77c4413f902523646be0d7f5520631df53ff6"},"path_in_vcs":""}"#;
        // Incompressible filler: the compressed archive stays megabytes.
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let filler = (0..2 * 1024 * 1024)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect::<Vec<_>>();
        let budget = super::super::types::test_budget();
        for (name, first) in [("streamed-first", true), ("streamed-last", false)] {
            let entry = format!("{name}-1.0.0/.cargo_vcs_info.json");
            let source = format!("{name}-1.0.0/src/lib.rs");
            let entries: Vec<(&str, &[u8], u8)> = if first {
                vec![
                    (entry.as_str(), &info[..], b'0'),
                    (source.as_str(), &filler, b'0'),
                ]
            } else {
                vec![
                    (source.as_str(), &filler, b'0'),
                    (entry.as_str(), &info[..], b'0'),
                ]
            };
            let http = super::super::test_http::ChunkedHttp::new(test_archive(&entries), 4096);
            let client = RegistryClient::uncached(&http, &budget);
            assert_eq!(
                crate_vcs(name, "1.0.0", &client).await,
                Some(("b6a77c4413f902523646be0d7f5520631df53ff6".to_owned(), None))
            );
            if first {
                assert!(
                    http.fed() <= 2,
                    "read {} of {} pieces",
                    http.fed(),
                    http.pieces()
                );
            } else {
                assert!(
                    http.fed() + 1 >= http.pieces(),
                    "read {} of {}",
                    http.fed(),
                    http.pieces()
                );
            }
        }
    }
}
