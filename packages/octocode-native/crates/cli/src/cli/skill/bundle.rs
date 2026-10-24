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
    pub fn text(&self) -> &'static str {
        self.instructions
    }

    pub fn matches(&self, destination: &Path) -> bool {
        self.compare(destination, false).unwrap_or(false)
    }

    pub fn freshness(&self, destination: &Path) -> Option<&'static str> {
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

    pub fn write(&self, destination: &Path) -> io::Result<()> {
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
mod tests {
    use super::*;

    #[test]
    fn every_embedded_skill_is_valid_and_round_trips() {
        assert!(!SKILLS.is_empty());
        for skill in SKILLS {
            assert_eq!(declared_name(skill.text()).expect(skill.name), skill.name);
            assert!(!skill.description.trim().is_empty(), "{}", skill.name);
            assert!(
                skill.text().lines().any(|line| {
                    line.strip_prefix("description:")
                        .and_then(|value| serde_json::from_str::<String>(value.trim()).ok())
                        .is_some_and(|decoded| decoded == skill.description)
                }),
                "{} embeds a description its frontmatter does not hold",
                skill.name
            );
            let directory = tempfile::tempdir().expect("tempdir");
            skill.write(directory.path()).expect("extract");
            assert!(skill.matches(directory.path()), "{}", skill.name);
            fs::write(directory.path().join("unexpected.txt"), "user file").expect("write");
            assert!(!skill.matches(directory.path()));
        }
    }

    #[test]
    fn declared_name_reads_one_plain_or_double_quoted_line() {
        let read = |header: &str| declared_name(&format!("---\n{header}\n---\nbody"));
        assert_eq!(
            read("name: demo-skill\ndescription: \"x\""),
            Ok("demo-skill".into())
        );
        assert_eq!(
            read("description: \"x\"\nname:   \"demo-2\"  "),
            Ok("demo-2".into())
        );
        for header in [
            "description: \"x\"",
            "name: |-\n  demo",
            "name: 'demo'",
            "name: demo # note",
            "name: \"demo",
            "name: \"de\\u006do\"",
            "name: demo\nname: other",
            "name:",
        ] {
            assert_eq!(read(header), Err(NAME_FORM.into()), "{header}");
        }
        assert!(declared_name("name: demo").is_err());
        assert!(declared_name("---\nname: demo\n").is_err());
    }

    #[test]
    fn freshness_detects_same_size_edits_and_ignores_finder_metadata() {
        let skill = &SKILLS[0];
        let directory = tempfile::tempdir().expect("tempdir");
        skill.write(directory.path()).expect("write");
        fs::write(directory.path().join(".DS_Store"), "finder").expect("metadata");
        assert_eq!(skill.freshness(directory.path()), Some("fresh"));
        assert!(
            !skill.matches(directory.path()),
            "installation equality protects extra files"
        );
        let path = directory.path().join("SKILL.md");
        let original = fs::read(&path).expect("read");
        let mut edited = original.clone();
        edited[0] ^= 1;
        fs::write(&path, edited).expect("edit");
        assert_eq!(skill.freshness(directory.path()), Some("stale"));
        fs::write(path, original).expect("restore");
        assert_eq!(skill.freshness(directory.path()), Some("fresh"));
        fs::remove_file(directory.path().join("SKILL.md")).expect("remove");
        assert_eq!(skill.freshness(directory.path()), Some("stale"));
    }

    #[cfg(unix)]
    #[test]
    fn cyclic_or_missing_trees_have_unknown_freshness() {
        let skill = &SKILLS[0];
        let directory = tempfile::tempdir().expect("tempdir");
        skill.write(directory.path()).expect("write");
        std::os::unix::fs::symlink(directory.path(), directory.path().join("cycle"))
            .expect("cycle");
        assert_eq!(skill.freshness(directory.path()), None);
        assert_eq!(skill.freshness(&directory.path().join("missing")), None);
    }
}
