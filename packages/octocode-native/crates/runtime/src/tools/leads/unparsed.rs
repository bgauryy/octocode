//! The files a walk passes over because no grammar parses their names, and
//! the notice that names them: one record and one sentence for every tool
//! that reads source through grammars (astSearch's scan, graphBuild's
//! build), each saying in its own [`Reader`] what an unparsed file costs.
use serde_json::Value;
use std::path::Path;

/// Example paths a file-count warning names.
pub(crate) const NAMED_EXAMPLES: usize = 3;

/// A tally of files a warning names: their count and the first
/// [`NAMED_EXAMPLES`] paths in path order, spelled as the response's row
/// paths are (workspace-relative, else absolute).
#[derive(Default)]
pub(crate) struct NamedFiles {
    pub(crate) count: u32,
    pub(crate) examples: std::collections::BTreeSet<String>,
}

impl NamedFiles {
    pub(crate) fn record(&mut self, path: String) {
        self.count = self.count.saturating_add(1);
        self.examples.insert(path);
        if self.examples.len() > NAMED_EXAMPLES {
            self.examples.pop_last();
        }
    }

    /// The same tally with each path respelled by `spell`.
    pub(crate) fn respelled(&self, spell: impl Fn(&str) -> String) -> Self {
        Self {
            count: self.count,
            examples: self.examples.iter().map(|path| spell(path)).collect(),
        }
    }

    /// The paths a warning lists: `a, b, c, …` when more files follow.
    pub(crate) fn listed(&self) -> String {
        let more = if self.count as usize > self.examples.len() {
            ", …"
        } else {
            ""
        };
        format!(
            "{}{more}",
            self.examples.iter().cloned().collect::<Vec<_>>().join(", ")
        )
    }
}

/// How one tool reads source through grammars: what its notice calls the
/// grammar a file lacks, and what passing over one file, or several, means
/// for its answer.
pub(crate) struct Reader {
    pub(crate) grammar: &'static str,
    /// The consequence clause, for one file and for several.
    pub(crate) unread: [&'static str; 2],
}

/// The files a walk passed over because no grammar parses their names.
#[derive(Default)]
pub(crate) struct Unparsed {
    /// Every such file.
    pub(crate) files: NamedFiles,
    /// Of those, the files of no known type (ripgrep's default type table):
    /// the ones that could hold source of a rule's language.
    pub(crate) untyped: NamedFiles,
    /// The rest, counted by type name.
    pub(crate) typed: std::collections::BTreeMap<String, usize>,
}

impl Unparsed {
    /// Record `path` (its row spelling `shown`), by the type its name is of
    /// ([`known_type`]), if any.
    pub(crate) fn record(&mut self, path: &Path, shown: String) {
        if let Some(known) = known_type(path) {
            *self.typed.entry(known.to_owned()).or_default() += 1;
        } else {
            self.untyped.record(shown.clone());
        }
        self.files.record(shown);
    }

    /// What a page discloses: under one rule language (`parser`, or the
    /// grammar inferred from the scan) a file of a known other type is not
    /// that language's source, so it is a quiet count beside the grammars
    /// the query does not compile in, and only a file of unknown type warns;
    /// without one language every such file warns.
    pub(crate) fn disclosed(
        &self,
        reader: &Reader,
        one_language: bool,
        mut skipped_grammars: Option<Value>,
    ) -> (Option<String>, Option<Value>) {
        if !one_language {
            return (unparsed_notice(reader, &self.files), skipped_grammars);
        }
        if !self.typed.is_empty() {
            let counts = skipped_grammars.get_or_insert_with(|| Value::Object(Default::default()));
            for (name, files) in &self.typed {
                let known = counts[name.as_str()].as_u64().unwrap_or_default();
                counts[name.as_str()] = Value::from(known.saturating_add(*files as u64));
            }
        }
        (unparsed_notice(reader, &self.untyped), skipped_grammars)
    }
}

/// The notice that names the files passed over for want of a grammar, what
/// that means for the `reader`'s answer, and the field that parses them.
pub(crate) fn unparsed_notice(reader: &Reader, files: &NamedFiles) -> Option<String> {
    let names = files.listed();
    let Reader {
        grammar,
        unread: [one, several],
    } = reader;
    match files.count {
        0 => None,
        1 => Some(format!(
            "1 file skipped: no {grammar} for its name ({names}); {one}; if it is source code, parserGlobs maps its name to a grammar."
        )),
        count => Some(format!(
            "{count} files skipped: no {grammar} for their names ({names}); {several}; if they are source code, parserGlobs maps their names to a grammar."
        )),
    }
}

/// The type `path`'s name is of in ripgrep's default type table (the one
/// localSearch's `fileType` selects from), if any; of several
/// (`CMakeLists.txt` is `cmake` and `txt`), the first by name, so the count
/// is the same in every process. The table is the `ignore` crate's constant
/// and its globs compile; a glob that did not would only leave its names of
/// no known type, so they warn, never the reverse.
fn known_type(path: &Path) -> Option<&'static str> {
    static TYPES: std::sync::LazyLock<(Vec<String>, Vec<usize>, globset::GlobSet)> =
        std::sync::LazyLock::new(|| {
            let mut table = ignore::types::TypesBuilder::new();
            table.add_defaults();
            let (mut names, mut owners, mut set) = (vec![], vec![], globset::GlobSetBuilder::new());
            // Sorted by name, so the lowest owner is the first name.
            for (index, def) in table.definitions().into_iter().enumerate() {
                for glob in def.globs() {
                    if let Ok(glob) = globset::GlobBuilder::new(glob)
                        .literal_separator(true)
                        .build()
                    {
                        set.add(glob);
                        owners.push(index);
                    }
                }
                names.push(def.name().to_owned());
            }
            let set = set.build().unwrap_or_else(|_| globset::GlobSet::empty());
            (names, owners, set)
        });
    let (names, owners, set) = &*TYPES;
    let name = path.file_name()?;
    let owner = set
        .matches(name)
        .into_iter()
        .filter_map(|glob| owners.get(glob).copied())
        .min()?;
    names.get(owner).map(String::as_str)
}
