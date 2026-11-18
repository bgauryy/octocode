use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(super) struct EmbeddedFile {
    pub path: &'static str,
    pub bytes: &'static [u8],
    #[cfg_attr(
        not(unix),
        expect(dead_code, reason = "only Unix applies executable permission bits")
    )]
    pub executable: bool,
}

#[derive(Debug)]
pub(super) struct EmbeddedSkill {
    pub name: &'static str,
    pub instructions: &'static str,
    /// Decoded from the one-line JSON string in the frontmatter when the bundle is generated.
    pub description: &'static str,
    pub files: &'static [EmbeddedFile],
}

include!(concat!(env!("OUT_DIR"), "/skills.rs"));

impl EmbeddedSkill {
    pub(super) fn text(&self) -> &'static str {
        self.instructions
    }

    pub(super) fn matches(&self, destination: &Path) -> bool {
        self.compare(destination, false).unwrap_or(false)
    }

    pub(super) fn freshness(&self, destination: &Path) -> Option<&'static str> {
        self.compare(destination, true)
            .ok()
            .map(|equal| if equal { "fresh" } else { "stale" })
    }

    fn compare(&self, destination: &Path, freshness: bool) -> io::Result<bool> {
        fn entries(
            root: &Path,
            path: &Path,
            freshness: bool,
            found: &mut BTreeMap<String, u64>,
            ancestors: &mut BTreeSet<PathBuf>,
        ) -> io::Result<()> {
            let real = path.canonicalize()?;
            if !ancestors.insert(real.clone()) {
                return Err(io::Error::other("skill directory link cycle"));
            }
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                if freshness && entry.file_name() == ".DS_Store" {
                    continue;
                }
                let metadata = if freshness {
                    fs::metadata(entry.path())?
                } else {
                    fs::symlink_metadata(entry.path())?
                };
                let relative = octocode_engine::text::slash_path(
                    entry.path().strip_prefix(root).map_err(io::Error::other)?,
                );
                if metadata.is_dir() {
                    if !freshness {
                        found.insert(format!("{relative}/"), 0);
                    }
                    entries(root, &entry.path(), freshness, found, ancestors)?;
                } else if metadata.is_file() {
                    found.insert(relative, metadata.len());
                } else {
                    return Err(io::Error::other("unexpected link or special file"));
                }
            }
            ancestors.remove(&real);
            Ok(())
        }
        if !freshness && !fs::symlink_metadata(destination)?.is_dir() {
            return Ok(false);
        }
        let mut found = BTreeMap::new();
        entries(
            destination,
            destination,
            freshness,
            &mut found,
            &mut BTreeSet::new(),
        )?;
        let mut expected = BTreeMap::new();
        for file in self.files {
            expected.insert(file.path.to_owned(), file.bytes.len() as u64);
            if !freshness {
                let mut parent = Path::new(file.path).parent();
                while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
                    expected.insert(format!("{}/", octocode_engine::text::slash_path(path)), 0);
                    parent = path.parent();
                }
            }
        }
        if found != expected {
            return Ok(false);
        }
        for file in self.files {
            if fs::read(destination.join(file.path))? != file.bytes {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn write(&self, destination: &Path) -> io::Result<()> {
        for file in self.files {
            let path = destination.join(file.path);
            fs::create_dir_all(
                path.parent()
                    .ok_or_else(|| io::Error::other("file has no parent"))?,
            )?;
            fs::write(&path, file.bytes)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    path,
                    fs::Permissions::from_mode(if file.executable { 0o755 } else { 0o644 }),
                )?;
            }
        }
        Ok(())
    }
}

pub(super) fn valid_name(name: &str) -> bool {
    name.as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// The frontmatter `name`, read from its single `name:` line without a YAML parser.
///
/// The Agent Skills spec limits names to a plain token, so the value is either bare or in double
/// quotes. Any other form (block scalar, single quotes, trailing comment) asks for a name override.
pub(super) fn declared_name(text: &str) -> Result<String, String> {
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Err("SKILL.md must begin with a --- frontmatter block".into());
    }
    let mut names = Vec::new();
    let mut closed = false;
    for line in lines {
        if line == "---" {
            closed = true;
            break;
        }
        if let Some(value) = line.strip_prefix("name:") {
            names.push(value.trim());
        }
    }
    if !closed {
        return Err("SKILL.md frontmatter is not closed".into());
    }
    let [value] = names.as_slice() else {
        return Err(NAME_FORM.into());
    };
    let name = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value);
    if valid_name(name) {
        Ok(name.into())
    } else {
        Err(NAME_FORM.into())
    }
}

const NAME_FORM: &str = "SKILL.md needs one `name: token` line (bare or double-quoted letters, digits, `.`, `_`, `-`); provide a name override to install it";

#[cfg(test)]
mod tests;
