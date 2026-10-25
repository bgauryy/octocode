use super::{cargo_workspace_root, lexical, manifest};
use std::path::{Path, PathBuf};

pub(in super::super) struct TargetRoots {
    directory: PathBuf,
    manifest: toml::Table,
    edition_2015: bool,
    build_script: Option<PathBuf>,
}

impl TargetRoots {
    pub(in super::super) fn new(directory: &Path, build_script: Option<PathBuf>) -> Self {
        let table = manifest(directory).unwrap_or_default();
        let edition = table
            .get("package")
            .and_then(|package| package.get("edition"));
        let inherited = edition
            .and_then(|edition| edition.get("workspace"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(false);
        let workspace = inherited
            .then(|| cargo_workspace_root(directory).and_then(|root| manifest(&root)))
            .flatten();
        let edition = if inherited {
            workspace
                .as_ref()
                .and_then(|table| table.get("workspace")?.get("package")?.get("edition"))
                .and_then(toml::Value::as_str)
        } else {
            edition.and_then(toml::Value::as_str)
        };
        let edition_2015 = edition.unwrap_or("2015") == "2015";
        Self {
            directory: directory.to_path_buf(),
            manifest: table,
            edition_2015,
            build_script,
        }
    }

    pub(in super::super) fn contains(&self, path: &Path) -> bool {
        if self.build_script.as_deref() == Some(path) {
            return true;
        }
        let library = self.manifest.get("lib");
        if (library.is_some() || self.automatic("lib", "autolib"))
            && lexical(
                &self.directory.join(
                    library
                        .and_then(|lib| lib.get("path"))
                        .and_then(toml::Value::as_str)
                        .unwrap_or("src/lib.rs"),
                ),
            ) == path
        {
            return true;
        }
        for (kind, directory, setting) in [
            ("bin", "src/bin", "autobins"),
            ("test", "tests", "autotests"),
            ("bench", "benches", "autobenches"),
            ("example", "examples", "autoexamples"),
        ] {
            let name = if kind == "bin" && path == self.directory.join("src/main.rs") {
                self.manifest
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(toml::Value::as_str)
            } else {
                target_name(path, &self.directory.join(directory))
            };
            let targets = self.manifest.get(kind).and_then(toml::Value::as_array);
            let mut configured = false;
            for target in targets.into_iter().flatten() {
                let matching_name =
                    name.is_some() && target.get("name").and_then(toml::Value::as_str) == name;
                configured |= matching_name;
                if let Some(source) = target.get("path").and_then(toml::Value::as_str) {
                    if lexical(&self.directory.join(source)) == path {
                        return true;
                    }
                } else if matching_name {
                    return true;
                }
            }
            if name.is_some() && !configured && self.automatic(kind, setting) {
                return true;
            }
        }
        false
    }

    fn automatic(&self, kind: &str, setting: &str) -> bool {
        self.manifest
            .get("package")
            .and_then(|p| p.get(setting))
            .and_then(toml::Value::as_bool)
            .unwrap_or(kind == "lib" || !self.edition_2015 || !self.manifest.contains_key(kind))
    }
}

fn target_name<'a>(path: &'a Path, directory: &Path) -> Option<&'a str> {
    let relative = path.strip_prefix(directory).ok()?;
    let mut components = relative.iter();
    let first = components.next()?;
    match (components.next(), components.next()) {
        (None, None) if relative.extension().is_some_and(|ext| ext == "rs") => {
            relative.file_stem()?.to_str()
        }
        (Some(file), None) if file == "main.rs" => first.to_str(),
        _ => None,
    }
}
