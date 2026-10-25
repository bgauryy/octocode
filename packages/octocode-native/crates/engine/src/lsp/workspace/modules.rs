use super::{cargo::TargetRoots, lexical};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

enum Declaration {
    Named(String),
    Path(PathBuf),
}

struct Source {
    path: PathBuf,
    declarations: Vec<Declaration>,
}

#[derive(Default)]
pub(super) struct ModuleSources(Vec<Source>);

impl ModuleSources {
    pub(super) fn push(&mut self, path: &Path, text: &str) {
        let mut declarations = Vec::new();
        let mut attribute_path = None;
        for line in text.lines() {
            let code = line.split("//").next().unwrap_or(line).trim();
            if code.is_empty() {
                continue;
            }
            if let Some(rest) = code.strip_prefix("#[path") {
                attribute_path = rest
                    .trim_start()
                    .strip_prefix('=')
                    .and_then(|value| value.trim_start().strip_prefix('"'))
                    .and_then(|value| value.split('"').next());
                continue;
            }
            if code.starts_with("#[") {
                continue;
            }
            let pending = attribute_path.take();
            if let Some(name) = module_declaration(code) {
                declarations.push(match pending {
                    Some(target) => {
                        Declaration::Path(lexical(&path.parent().unwrap_or(path).join(target)))
                    }
                    None => Declaration::Named(name.to_owned()),
                });
            }
        }
        self.0.push(Source {
            path: lexical(path),
            declarations,
        });
    }

    /// Ordinary modules use a stem directory; roots, mod.rs and #[path]
    /// contexts use the parent. A source may be loaded in both contexts.
    /// Each context is visited once, without rereading any scanned source.
    pub(super) fn declared_by(&self, anchor: &Path, targets: &TargetRoots) -> Vec<PathBuf> {
        let sources: HashMap<_, _> = self
            .0
            .iter()
            .enumerate()
            .map(|(index, source)| (source.path.as_path(), index))
            .collect();
        let attributed: HashSet<_> = self
            .0
            .iter()
            .flat_map(|source| &source.declarations)
            .filter_map(|declaration| match declaration {
                Declaration::Path(path) => Some(path.as_path()),
                Declaration::Named(_) => None,
            })
            .collect();
        let mut seen = HashSet::new();
        let mut queue = VecDeque::new();
        for (index, source) in self.0.iter().enumerate() {
            let parent = targets.contains(&source.path)
                || is_mod_rs(&source.path)
                || attributed.contains(source.path.as_path());
            seen.insert((index, parent));
            queue.push_back((index, parent));
        }
        let mut owners = HashSet::new();
        while let Some((index, parent)) = queue.pop_front() {
            let source = &self.0[index];
            let directory = source.path.parent().unwrap_or(&source.path);
            let base = if parent {
                directory.to_path_buf()
            } else {
                directory.join(source.path.file_stem().unwrap_or_default())
            };
            for declaration in &source.declarations {
                let (paths, attributed) = match declaration {
                    Declaration::Path(path) => (vec![path.clone()], true),
                    Declaration::Named(name) => (
                        vec![
                            lexical(&base.join(format!("{name}.rs"))),
                            lexical(&base.join(name).join("mod.rs")),
                        ],
                        false,
                    ),
                };
                for path in paths {
                    if path == anchor {
                        owners.insert(index);
                    }
                    if let Some(&child) = sources.get(path.as_path()) {
                        let context = (child, attributed || is_mod_rs(&path));
                        if seen.insert(context) {
                            queue.push_back(context);
                        }
                    }
                }
            }
        }
        self.0
            .iter()
            .enumerate()
            .filter(|(index, _)| owners.contains(index))
            .map(|(_, source)| source.path.clone())
            .collect()
    }
}

fn is_mod_rs(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "mod.rs")
}

/// The module name of an out-of-line declaration `[pub[(…)]] mod name;`.
fn module_declaration(code: &str) -> Option<&str> {
    let rest = match code.strip_prefix("pub") {
        Some(rest) => {
            let rest = rest.trim_start();
            match rest.strip_prefix('(') {
                Some(scoped) => scoped.split_once(')')?.1.trim_start(),
                None => rest,
            }
        }
        None => code,
    };
    let name = rest.strip_prefix("mod ")?.trim().strip_suffix(';')?.trim();
    let name = name.strip_prefix("r#").unwrap_or(name);
    (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')).then_some(name)
}
