//! The copy of a project's compilation database a background index reads, kept
//! under the Octocode home: what goes in it, and when it is written.
use super::*;

mod entry;
use entry::{Scans, stays_in_project, without_secrets};

/// Point clangd at a copy of the project's database under `home` and turn its
/// background index on. No effect without such a database, when the user chose
/// the index themselves, or when `compilationDatabasePath` overrides the flag.
/// [`prepare_mirror`] writes the copy when the server starts.
pub(crate) fn mirror_database(config: &mut LspLanguageServerConfig, home: &Path) {
    let args = config.args.get_or_insert_with(Vec::new);
    if args.iter().any(|arg| is_background_index_flag(arg))
        || config
            .initialization_options
            .as_ref()
            .is_some_and(|options| options.get("compilationDatabasePath").is_some())
    {
        return;
    }
    let Some(database) = project_database(config) else {
        return;
    };
    let database = database.canonicalize().unwrap_or(database);
    let digest = crate::digest::sha256(database.to_string_lossy().as_bytes());
    let mirror = home
        .join("lsp-workspaces")
        .join("clangd")
        .join(&digest[..16]);
    let mut kept = Vec::new();
    let mut args = config.args.take().unwrap_or_default().into_iter();
    while let Some(arg) = args.next() {
        let flag = arg.strip_prefix("--").or_else(|| arg.strip_prefix('-'));
        match flag.and_then(|flag| flag.strip_prefix("compile-commands-dir")) {
            Some("") => {
                args.next();
            }
            Some(value) if value.starts_with('=') => {}
            _ => kept.push(arg),
        }
    }
    kept.push(format!("--compile-commands-dir={}", mirror.display()));
    kept.push("--background-index".to_owned());
    config.args = Some(kept);
    config.env.get_or_insert_with(Default::default).insert(
        MIRRORED_DATABASE.to_owned(),
        database.to_string_lossy().into_owned(),
    );
}

/// Write the mirror [`mirror_database`] named, when its copy differs. A relative
/// `directory` resolves against the database's own folder, so it is made absolute
/// here; an entry that would read outside the project is left out
/// ([`stays_in_project`]), and no entry keeps a secret ([`without_secrets`]). The
/// folder is owner-only, which also covers the index clangd writes in it at its own
/// default modes. An error stops the start: without its copy clangd would fall back
/// to the project's database and write its index into the repository.
pub(crate) fn prepare_mirror(config: &LspLanguageServerConfig) -> std::io::Result<()> {
    let Some(database) = config
        .env
        .as_ref()
        .and_then(|env| env.get(MIRRORED_DATABASE))
    else {
        return Ok(());
    };
    let Some(mirror) = compile_commands_dir(config.args.as_deref().unwrap_or_default()) else {
        return Ok(());
    };
    let database = Path::new(database);
    let origin = database.parent().unwrap_or(database);
    let root = Path::new(&config.workspace_root);
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut scans = Scans::default();
    let entries = serde_json::from_slice::<Vec<serde_json::Value>>(&std::fs::read(database)?)
        .unwrap_or_default()
        .into_iter()
        .filter(serde_json::Value::is_object)
        .filter_map(|mut entry| {
            let directory = origin.join(entry["directory"].as_str().unwrap_or_default());
            entry["directory"] = serde_json::json!(directory.to_string_lossy());
            stays_in_project(&entry, &directory, &root)
                .then(|| without_secrets(&entry, &mut scans))
                .flatten()
        })
        .collect::<Vec<_>>();
    let content = serde_json::to_vec(&entries).map_err(std::io::Error::other)?;
    let mirror = Path::new(&mirror);
    create_owner_only_dir(mirror)?;
    let target = mirror.join("compile_commands.json");
    if is_owner_only_file(&target) && std::fs::read(&target).is_ok_and(|current| current == content)
    {
        return Ok(());
    }
    octocode_io::atomic::write_atomic(&target, &content, false)
}

/// The mirror's folder, created or narrowed to its owner: the folder is Octocode's
/// own, and everything clangd writes in it stays readable to no one else.
fn create_owner_only_dir(path: &Path) -> std::io::Result<()> {
    octocode_io::atomic::create_private_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Whether `path` is a regular file no other user can read: a link, or a copy at
/// wider modes, is written again.
fn is_owner_only_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| {
        #[cfg(unix)]
        let private = {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode() & 0o077 == 0
        };
        #[cfg(not(unix))]
        let private = true;
        meta.file_type().is_file() && private
    })
}

#[cfg(test)]
mod tests;
