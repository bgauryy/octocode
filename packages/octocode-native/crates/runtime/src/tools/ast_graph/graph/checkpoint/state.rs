use super::*;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum Stage {
    Extracting,
    Linking,
    Complete,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Manifest {
    pub(super) version: u32,
    pub(super) namespace: String,
    pub(super) inventory_digest: String,
    pub(super) config_digest: String,
    pub(super) registry_identity: String,
    pub(super) stage: Stage,
}

impl Manifest {
    pub(super) fn read(dir: &Path) -> Option<Self> {
        let bytes = crate::tools::source::read_bounded(&dir.join("job.json"), 4096).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
    pub(super) fn write(&self, dir: &Path, stage: Stage) -> Result<(), ToolError> {
        let manifest = Self {
            stage,
            ..self.clone()
        };
        let bytes = serde_json::to_vec(&manifest).map_err(|error| failure(error.to_string()))?;
        durable_write(&dir.join("job.json"), &bytes)
    }
}

pub(super) fn checked_path(
    paths: &PathPolicy,
    target: &Path,
    output: bool,
) -> Result<(), ToolError> {
    let checked = if output {
        paths.validate_output(target)
    } else {
        paths.validate_read(target)
    }
    .map_err(ToolError::policy)?;
    if checked.canonical != target
        || std::fs::symlink_metadata(target).is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(failure("graph checkpoint paths must not be symlinks"));
    }
    Ok(())
}

pub(in crate::tools::ast_graph) fn job_path(base: &Path, root: &Path) -> PathBuf {
    base.join("jobs").join(octocode_engine::digest::sha256(
        root.to_string_lossy().as_bytes(),
    ))
}

impl<'a> Job<'a> {
    pub(in crate::tools::ast_graph::graph) fn open(
        scan: &GraphScan,
        root: &Path,
        inventory: &[octocode_engine::types::FileSystemEntry],
        paths: &'a PathPolicy,
        security: &'a ContentSecurity,
        registry: RegistryView<'_>,
    ) -> Result<Option<Self>, ToolError> {
        let Some(base) = &scan.checkpoint else {
            return Ok(None);
        };
        if std::fs::symlink_metadata(base).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(failure("graph checkpoint directory must not be a symlink"));
        }
        let base = paths
            .validate_output(base)
            .map_err(ToolError::policy)?
            .canonical;
        let dir = job_path(&base, root);
        checked_path(paths, &dir, true)?;
        let registry_identity = super::super::registry_identity(registry)?;
        let identity = serde_json::to_vec(&(
            JOB_VERSION,
            cache::namespace(scan, root, registry)?,
            &scan.exclude_dir,
            scan.max_files,
            scan.cargo_metadata,
            &scan.cargo,
            super::super::super::ANALYSIS_VERSION,
        ))
        .map_err(|error| failure(error.to_string()))?;
        let mut names = inventory
            .iter()
            .map(|entry| entry.relative_path.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        let configs = super::super::super::freshness::config_inputs(
            paths,
            root,
            &super::super::super::freshness::scope_dirs(root, names.iter().copied()),
        );
        let inventory_digest = octocode_engine::digest::sha256(
            &serde_json::to_vec(&names).map_err(|error| failure(error.to_string()))?,
        );
        let config_digest = octocode_engine::digest::sha256(
            &serde_json::to_vec(&configs).map_err(|error| failure(error.to_string()))?,
        );
        let manifest = Manifest {
            version: JOB_VERSION,
            namespace: octocode_engine::digest::sha256(&identity),
            inventory_digest,
            config_digest,
            registry_identity,
            stage: Stage::Extracting,
        };
        let manifest_path = dir.join("job.json");
        checked_path(paths, &manifest_path, true)?;
        if std::fs::symlink_metadata(&manifest_path).is_ok_and(|metadata| !metadata.is_file()) {
            return Err(failure("graph checkpoint manifest must be a regular file"));
        }
        let previous = Manifest::read(&dir);
        let compatible = previous
            .as_ref()
            .is_some_and(|old| old.version == JOB_VERSION && old.namespace == manifest.namespace);
        let resumed = !scan.force
            && compatible
            && previous.as_ref().is_some_and(|old| {
                old.stage != Stage::Complete
                    && old.inventory_digest == manifest.inventory_digest
                    && old.config_digest == manifest.config_digest
            });
        if dir.exists() && (scan.force || !compatible) {
            std::fs::remove_dir_all(&dir)
                .map_err(|error| failure(format!("cannot reset graph checkpoint: {error}")))?;
        }
        octocode_io::atomic::create_private_dir_all(&dir)
            .map_err(|error| failure(error.to_string()))?;
        for parent in dir.ancestors().skip(1).take(3) {
            sync_dir(parent)?;
        }
        let expected = names
            .iter()
            .map(|name| {
                (
                    format!("{}.fact", octocode_engine::digest::sha256(name.as_bytes())),
                    *name,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut state = State::default();
        for entry in std::fs::read_dir(&dir).map_err(|error| failure(error.to_string()))? {
            let entry = entry.map_err(|error| failure(error.to_string()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "job.json" {
                continue;
            }
            let kind = entry
                .file_type()
                .map_err(|error| failure(error.to_string()))?;
            if !kind.is_file() {
                return Err(failure("unexpected non-file in graph checkpoint"));
            }
            if let Some(relative) = expected.get(&name) {
                let bytes = entry
                    .metadata()
                    .map_err(|error| failure(error.to_string()))?
                    .len();
                state.bytes = state.bytes.saturating_add(bytes);
                state.artifacts.insert((*relative).into(), bytes);
            } else {
                std::fs::remove_file(entry.path()).map_err(|error| failure(error.to_string()))?;
            }
        }
        if state.bytes > LIMITS.spill_bytes {
            return Err(limit("checkpoint disk artifacts", LIMITS.spill_bytes));
        }
        manifest.write(&dir, Stage::Extracting)?;
        Ok(Some(Self {
            dir,
            manifest,
            resumed,
            state: Mutex::new(state),
            paths,
            security,
        }))
    }
}

pub(super) fn durable_write(path: &Path, bytes: &[u8]) -> Result<(), ToolError> {
    octocode_io::atomic::write_atomic(path, bytes, true)
        .map_err(|error| failure(format!("cannot commit graph checkpoint: {error}")))?;
    if let Some(parent) = path.parent() {
        sync_dir(parent)?;
    }
    Ok(())
}

pub(super) fn sync_dir(path: &Path) -> Result<(), ToolError> {
    #[cfg(unix)]
    std::fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| failure(format!("cannot sync graph checkpoint directory: {error}")))?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
