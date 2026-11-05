//! Java imports: a class by its package path under any source root, and what an
//! import no scanned file answers says about where the class is.
use super::{PackageIndex, PackageLink, closeness};

impl PackageIndex {
    pub(super) fn resolve_java(&self, spec: &str, importer: &str) -> PackageLink {
        // `import static a.b.C.*;` names class `C`'s members, not a package.
        let (spec, static_member) = match spec.strip_prefix("static ") {
            Some(member) => (member.trim(), true),
            None => (spec.trim(), false),
        };
        if let Some(package) = spec.strip_suffix(".*").filter(|_| !static_member) {
            let tail = package.replace('.', "/");
            let best = self
                .files_by_dir
                .keys()
                .filter(|dir| *dir == &tail || dir.ends_with(&format!("/{tail}")))
                .max_by_key(|dir| closeness(dir, importer));
            return match best {
                Some(dir) => {
                    let files = self.package_files(dir, ".java");
                    if files.is_empty() {
                        PackageLink::UnresolvedInternal
                    } else {
                        PackageLink::Files(files)
                    }
                }
                None => self.java_unscanned(&tail, &tail),
            };
        }
        // `a.b.C`, `a.b.C.Inner`, `static a.b.C.member` and `static a.b.C.*`
        // all name the file of the longest prefix that is a scanned class.
        let spec = spec.strip_suffix(".*").unwrap_or(spec);
        let segments = spec.split('.').collect::<Vec<_>>();
        for end in (2..=segments.len()).rev() {
            let tail = format!("{}.java", segments[..end].join("/"));
            let name = format!("{}.java", segments[end - 1]);
            let best = self
                .java_by_name
                .get(&name)
                .into_iter()
                .flatten()
                .filter(|file| **file == tail || file.ends_with(&format!("/{tail}")))
                .max_by_key(|file| closeness(file, importer));
            if let Some(file) = best {
                return PackageLink::Files(vec![file.clone()]);
            }
            if let Some(file) = self.below_root_names(&tail) {
                return PackageLink::Files(vec![file]);
            }
        }
        // The package ends at the first segment named like a class, else
        // before the last one.
        let class = segments
            .iter()
            .position(|segment| segment.starts_with(|c: char| c.is_ascii_uppercase()))
            .unwrap_or(segments.len().saturating_sub(1));
        let package = segments[..class].join("/");
        let file = format!(
            "{}.java",
            segments[..=class.min(segments.len() - 1)].join("/")
        );
        self.java_unscanned(&package, &file)
    }

    /// The scanned file a Java class path names when the scan root lies inside
    /// its package path (`com/acme/B.java` under a scan of `.../com/acme`).
    fn below_root_names(&self, tail: &str) -> Option<String> {
        let mut below = String::new();
        self.java_root_names.iter().find_map(|name| {
            below = if below.is_empty() {
                name.clone()
            } else {
                format!("{name}/{below}")
            };
            let file = tail.strip_prefix(&below)?.strip_prefix('/')?;
            self.java_by_name
                .get(file.rsplit('/').next()?)?
                .iter()
                .find(|known| known.as_str() == file)
                .cloned()
        })
    }

    /// How an import no scanned file answers links: internal when the scan
    /// holds its `package` (the class may share another file, or be generated)
    /// or a file above the scan root spells `file`, else external.
    fn java_unscanned(&self, package: &str, file: &str) -> PackageLink {
        if self.java_packages.contains(package) || self.java_above_root(file) {
            PackageLink::UnresolvedInternal
        } else {
            PackageLink::External
        }
    }

    fn java_above_root(&self, path: &str) -> bool {
        if self.above_root.is_empty() {
            return false;
        }
        let mut hits = self
            .java_outer_hits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *hits
            .entry(path.to_owned())
            .or_insert_with(|| self.above_root.iter().any(|dir| dir.join(path).exists()))
    }
}
