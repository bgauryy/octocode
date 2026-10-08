//! Release refs for registries whose metadata names no commit. Each is a
//! lead source only: any failure leaves the lookup without a release ref.
use super::ArtifactType;
use super::http::RegistryClient;
use super::util::{commit_sha, encode_component, parse_url, string};
use serde_json::Value;
use std::io::Read;

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
/// none); `None` when the archive cannot be read.
async fn archive_vcs_info(name: &str, version: &str, client: &RegistryClient<'_>) -> Option<Value> {
    let url = parse_url(&format!(
        "https://static.crates.io/crates/{name}/{name}-{version}.crate",
        name = encode_component(name),
        version = encode_component(version),
    ))
    .ok()?;
    let archive = client
        .bytes(ArtifactType::Crates, url)
        .await
        .ok()
        .flatten()?;
    Some(
        archive_entry(&archive, &format!("{name}-{version}/.cargo_vcs_info.json"))
            .and_then(|info| serde_json::from_slice(&info).ok())
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
    )
}

/// One file's bytes from a gzip-compressed tar archive (POSIX ustar names
/// with prefixes, GNU long names).
fn archive_entry(archive: &[u8], wanted: &str) -> Option<Vec<u8>> {
    let mut tar = flate2::read::GzDecoder::new(archive).take(MAX_ARCHIVE_BYTES);
    let mut long_name: Option<String> = None;
    loop {
        let mut header = [0u8; 512];
        tar.read_exact(&mut header).ok()?;
        if header.iter().all(|byte| *byte == 0) {
            return None;
        }
        let field = |range: std::ops::Range<usize>| {
            let bytes = &header[range];
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len());
            String::from_utf8_lossy(&bytes[..end]).into_owned()
        };
        let size = u64::from_str_radix(field(124..136).trim(), 8).ok()?;
        let kind = header[156];
        let name = long_name.take().unwrap_or_else(|| {
            let prefix = field(345..500);
            let name = field(0..100);
            if &header[257..263] == b"ustar\0" && !prefix.is_empty() {
                format!("{prefix}/{name}")
            } else {
                name
            }
        });
        let mut body = Vec::new();
        let padded = size.div_ceil(512) * 512;
        let wanted_body = kind == b'L' || (name == wanted && matches!(kind, b'0' | 0));
        if wanted_body && size > MAX_ENTRY_BYTES {
            return None;
        }
        if wanted_body {
            (&mut tar).take(size).read_to_end(&mut body).ok()?;
            std::io::copy(&mut (&mut tar).take(padded - size), &mut std::io::sink()).ok()?;
        } else {
            std::io::copy(&mut (&mut tar).take(padded), &mut std::io::sink()).ok()?;
        }
        match kind {
            b'L' => {
                let end = body
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(body.len());
                long_name = Some(String::from_utf8_lossy(&body[..end]).into_owned());
            }
            _ if wanted_body => return Some(body),
            _ => {}
        }
    }
}

/// Largest sdist read to find where a release declares its dependencies.
const MAX_SDIST_BYTES: u64 = 8 * 1024 * 1024;

/// Where a PyPI release declares its dependencies, read from its sdist:
/// `setup.py` when the sdist's `pyproject.toml` declares no
/// `[project] dependencies` (absent, or only tool settings) and a
/// `setup.py` ships; `pyproject.toml` when it declares them. `None` (the
/// caller's default) without a readable `.tar.gz` sdist.
pub(crate) async fn pypi_sdist_manifest(
    name: &str,
    version: &str,
    files: &[Value],
    client: &RegistryClient<'_>,
) -> Option<&'static str> {
    let resource = format!("pypi:{name}@{version}:manifest");
    if let Some(fact) = client.fact(&resource) {
        return manifest_name(fact.as_str()?);
    }
    let sdist = files.iter().find(|file| {
        file.get("packagetype").and_then(Value::as_str) == Some("sdist")
            && file
                .get("size")
                .and_then(Value::as_u64)
                .is_some_and(|size| size <= MAX_SDIST_BYTES)
    })?;
    let url = sdist.get("url").and_then(Value::as_str)?;
    let root = url.rsplit('/').next()?.strip_suffix(".tar.gz")?.to_owned();
    let archive = client
        .bytes(ArtifactType::Pypi, parse_url(url).ok()?)
        .await
        .ok()
        .flatten()?;
    let pyproject = archive_entry(&archive, &format!("{root}/pyproject.toml"));
    let declared = pyproject
        .as_deref()
        .is_some_and(|text| declares_project_dependencies(&String::from_utf8_lossy(text)));
    let manifest = if !declared && archive_entry(&archive, &format!("{root}/setup.py")).is_some() {
        "setup.py"
    } else {
        "pyproject.toml"
    };
    client.remember_fact(&resource, &Value::from(manifest));
    manifest_name(manifest)
}

fn manifest_name(name: &str) -> Option<&'static str> {
    ["setup.py", "pyproject.toml"]
        .into_iter()
        .find(|known| *known == name)
}

/// A `pyproject.toml` whose `[project]` table lists `dependencies` (not
/// as `dynamic`, which defers them to the build backend's setup.py).
fn declares_project_dependencies(pyproject: &str) -> bool {
    let mut in_project = false;
    for line in pyproject.lines().map(str::trim) {
        if line.starts_with('[') {
            in_project = line == "[project]";
        } else if in_project
            && line
                .strip_prefix("dependencies")
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        {
            return true;
        }
    }
    false
}

/// A pending release-tag check.
pub type TagFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = Option<bool>> + 'a>>;

/// Checks that a ref (a tag or a commit SHA) exists in a GitHub repository,
/// through the configured GitHub API (its URL, the caller's credential and
/// request budget).
pub trait ReleaseTags: Send + Sync {
    /// `Some(true)` when `tag` names a commit of `owner/repo`, `Some(false)`
    /// when the repository has no such ref, `None` when the check failed.
    fn exists<'a>(&'a self, owner: &'a str, repo: &'a str, tag: &'a str) -> TagFuture<'a>;
}

/// The release tag of `version` in a GitHub repository, when one exists
/// upstream: `v<version>`, then `<version>`. Without a GitHub check, or
/// when the check fails (rate limit, missing repository), there is no tag.
pub(crate) async fn github_release_tag(
    repository: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Option<String> {
    let tags = client.tags?;
    let (owner, repo) = github_slug(repository)?;
    for tag in [format!("v{version}"), version.to_owned()] {
        if tags.exists(&owner, &repo, &tag).await? {
            return Some(tag);
        }
    }
    None
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
    tags.exists(&owner, &repo, reference).await == Some(false)
}

/// `owner/repo` of a `https://github.com/<owner>/<repo>…` URL.
fn github_slug(repository: &str) -> Option<(String, String)> {
    let url = url::Url::parse(repository).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    if host != "github.com" && host != "www.github.com" {
        return None;
    }
    let mut segments = url.path_segments()?.filter(|part| !part.is_empty());
    let owner = segments.next()?;
    let repo = segments.next()?.trim_end_matches(".git");
    (!repo.is_empty()).then(|| (owner.to_owned(), repo.to_owned()))
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

    /// Serves one archive and counts the downloads.
    struct ArchiveOnce {
        archive: Vec<u8>,
        downloads: std::sync::atomic::AtomicUsize,
    }

    impl super::super::ArtifactHttp for ArchiveOnce {
        fn get<'a>(
            &'a self,
            _request: super::super::http::ArtifactHttpRequest,
            _budget: &'a crate::providers::RequestBudget,
        ) -> super::super::http::ArtifactHttpFuture<'a> {
            self.downloads
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let body = self.archive.clone();
            Box::pin(
                async move { Ok(super::super::http::ArtifactHttpResponse { status: 200, body }) },
            )
        }
    }

    /// A published crate's VCS info is read from its archive once.
    #[tokio::test]
    async fn crate_vcs_info_is_downloaded_once() {
        let name = format!("vcs-once-{}", std::process::id());
        let info =
            br#"{"git":{"sha1":"b6a77c4413f902523646be0d7f5520631df53ff6"},"path_in_vcs":"x"}"#;
        let entry = format!("{name}-1.0.0/.cargo_vcs_info.json");
        let http = ArchiveOnce {
            archive: test_archive(&[(entry.as_str(), &info[..], b'0')]),
            downloads: std::sync::atomic::AtomicUsize::new(0),
        };
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
        assert_eq!(crate_vcs(&name, "1.0.0", &client).await, expected);
        assert_eq!(crate_vcs(&name, "1.0.0", &client).await, expected);
        assert_eq!(http.downloads.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    /// AR4: a setuptools sdist (tool-only pyproject.toml, setup.py ships)
    /// declares its dependencies in setup.py; a `[project] dependencies`
    /// pyproject.toml keeps pyproject.toml; a dynamic list defers to setup.py.
    #[tokio::test]
    async fn the_sdist_names_the_file_that_declares_dependencies() {
        let budget = super::super::types::test_budget();
        let files = |name: &str| {
            vec![
                serde_json::json!({"packagetype":"bdist_wheel","size":10,"url":"https://files/x.whl"}),
                serde_json::json!({"packagetype":"sdist","size":100,
                    "url":format!("https://files.pythonhosted.org/p/{name}-1.0.tar.gz")}),
            ]
        };
        for (pyproject, expected) in [
            (&b"[tool.black]\nline-length = 88\n"[..], "setup.py"),
            (
                &b"[project]\nname = \"x\"\ndependencies = [\"a>=1\"]\n"[..],
                "pyproject.toml",
            ),
            (
                &b"[project]\nname = \"x\"\ndynamic = [\"dependencies\"]\n"[..],
                "setup.py",
            ),
        ] {
            let name = format!("sdist-{}", std::process::id());
            let root = format!("{name}-1.0");
            let http = ArchiveOnce {
                archive: test_archive(&[
                    (format!("{root}/pyproject.toml").as_str(), pyproject, b'0'),
                    (format!("{root}/setup.py").as_str(), b"setup()", b'0'),
                ]),
                downloads: std::sync::atomic::AtomicUsize::new(0),
            };
            let client = RegistryClient::uncached(&http, &budget);
            assert_eq!(
                pypi_sdist_manifest(&name, "1.0", &files(&name), &client).await,
                Some(expected)
            );
        }
        // No sdist: the caller's default.
        let http = ArchiveOnce {
            archive: Vec::new(),
            downloads: std::sync::atomic::AtomicUsize::new(0),
        };
        let client = RegistryClient::uncached(&http, &budget);
        let wheel_only = [
            serde_json::json!({"packagetype":"bdist_wheel","size":10,"url":"https://files/x.whl"}),
        ];
        assert_eq!(
            pypi_sdist_manifest("x", "1.0", &wheel_only, &client).await,
            None
        );
        assert_eq!(http.downloads.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[test]
    fn github_slugs_come_only_from_github_urls() {
        assert_eq!(
            github_slug("https://github.com/psf/requests"),
            Some(("psf".into(), "requests".into()))
        );
        assert_eq!(
            github_slug("https://github.com/o/r.git/tree/main"),
            Some(("o".into(), "r".into()))
        );
        assert_eq!(github_slug("https://gitlab.com/o/r"), None);
        assert_eq!(github_slug("https://github.com/o"), None);
    }
}
