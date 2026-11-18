//! The files a walk passes over because no grammar parses their names, and
//! the notice that names them: one record and one sentence for every tool
//! that reads source through grammars (astSearch's scan, graphBuild's
//! build), each saying in its own [`Reader`] what an unparsed file costs.
//! The sentence names [`NAMED_EXAMPLES`] of the files it counts; every one of
//! them is a `skippedPaths` entry ([`Unparsed::skipped`]), so paging reaches
//! them all.
use super::skipped::SkippedEntry;
use octocode_contract_types::SkippedPathCause;
use serde_json::Value;
use std::path::Path;

/// Example paths a file-count warning names.
pub const NAMED_EXAMPLES: usize = 3;

/// A tally of files a warning names: their count and the first
/// [`NAMED_EXAMPLES`] paths in path order, spelled as the response's row
/// paths are (workspace-relative, else absolute).
#[derive(Clone, Default)]
pub struct NamedFiles {
    pub count: u32,
    pub(crate) examples: std::collections::BTreeSet<String>,
}

impl NamedFiles {
    pub fn record(&mut self, path: String) {
        self.count = self.count.saturating_add(1);
        self.examples.insert(path);
        if self.examples.len() > NAMED_EXAMPLES {
            self.examples.pop_last();
        }
    }

    /// This tally and `other`'s as one: both counts, the first examples of
    /// both in path order.
    fn merge(&mut self, other: &Self) {
        self.count = self.count.saturating_add(other.count);
        self.examples.extend(other.examples.iter().cloned());
        while self.examples.len() > NAMED_EXAMPLES {
            self.examples.pop_last();
        }
    }

    /// The same tally with each path respelled by `spell`.
    pub fn respelled(&self, spell: impl Fn(&str) -> String) -> Self {
        Self {
            count: self.count,
            examples: self.examples.iter().map(|path| spell(path)).collect(),
        }
    }

    /// The paths a warning lists: `a, b, c, …` when more files follow.
    pub fn listed(&self) -> String {
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
pub struct Reader {
    pub grammar: &'static str,
    /// The consequence clause, for one file and for several.
    pub unread: [&'static str; 2],
}

/// The files a walk passed over because no grammar parses their names.
#[derive(Default)]
pub struct Unparsed {
    /// Every such file, each once: its absolute path and the type its name
    /// is of ([`known_type`]), if any.
    passed: std::collections::BTreeMap<String, Option<&'static str>>,
    /// Every such file, as the warning counts and names them.
    pub files: NamedFiles,
    /// Of those, the files of no known type (ripgrep's default type table):
    /// they could hold source of any grammar.
    pub(crate) untyped: NamedFiles,
    /// The rest, by type name.
    pub(crate) typed: std::collections::BTreeMap<String, NamedFiles>,
}

/// The file types ([`known_type`]) of the names the grammars a scan reads
/// parse: a passed-over file of one of these types may hold their source,
/// while one of another known type (JSON, CSS, plain text) holds none.
pub struct SourceTypes(
    std::sync::OnceLock<std::collections::BTreeSet<&'static str>>,
    Option<String>,
);

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    /// Source-type tables this thread built ([`SourceTypes::types`]): the
    /// build forces the file-type table, which a scan that passed nothing
    /// over never needs.
    pub static TABLE_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whether a passed-over file of type `known` is one a page that `read`
/// these grammars' names warns about ([`Unparsed::disclosed`]): of no known
/// type, or of one of theirs; with no grammar read, every file.
fn warns(known: Option<&str>, read: Option<&SourceTypes>) -> bool {
    match (read, known) {
        (Some(sources), Some(known)) => sources.types().contains(known),
        _ => true,
    }
}

impl SourceTypes {
    /// The types of the names `selector`'s grammars parse
    /// ([`octocode_engine::signatures::languages::family_extensions`]); of
    /// every grammar's names without one.
    pub fn read_by(selector: Option<&str>) -> Self {
        Self(std::sync::OnceLock::new(), selector.map(str::to_owned))
    }

    /// The types themselves, worked out on first use: a page that passed no
    /// file over never asks, so it never builds the file-type table.
    fn types(&self) -> &std::collections::BTreeSet<&'static str> {
        use octocode_engine::signatures::languages;
        self.0.get_or_init(|| {
            #[cfg(any(test, feature = "test-support"))]
            TABLE_BUILDS.with(|builds| builds.set(builds.get() + 1));
            let extensions = self
                .1
                .as_deref()
                .and_then(languages::family_extensions)
                .unwrap_or_else(|| {
                    languages::all_entries()
                        .iter()
                        .flat_map(|entry| entry.extensions.iter().chain(entry.also_parses))
                        .map(|extension| extension.to_ascii_lowercase())
                        .collect()
                });
            extensions
                .iter()
                .filter_map(|extension| known_type(Path::new(&format!("source.{extension}"))))
                .collect()
        })
    }
}

/// One file a walk passed over, judged before it is recorded
/// ([`Unparsed::add`]): `None` for binary content, which is no source of any
/// language, so it leaves no absence unproven and is never named.
pub struct Passed {
    path: String,
    shown: String,
    known: Option<&'static str>,
}

impl Passed {
    /// `path` (its row spelling `shown`), by the type its name is of
    /// ([`known_type`]), if any; `None` when its leading bytes are binary
    /// ([`octocode_engine::text::sniff`]). A file that cannot be read stays,
    /// since nothing shows it is not source.
    pub fn of(path: &Path, shown: String) -> Option<Self> {
        if is_binary(path) {
            return None;
        }
        Some(Self {
            path: path.to_string_lossy().into_owned(),
            shown,
            known: known_type(path),
        })
    }
}

/// Whether the leading bytes of the regular file at `path` are binary.
fn is_binary(path: &Path) -> bool {
    use std::io::Read;
    let Ok((file, _)) = octocode_engine::text::open_regular(path) else {
        return false;
    };
    let mut head = Vec::new();
    file.take(octocode_engine::text::SNIFF_BYTES as u64)
        .read_to_end(&mut head)
        .is_ok_and(|_| {
            octocode_engine::text::sniff(&head) == octocode_engine::text::BinaryVerdict::Binary
        })
}

impl Unparsed {
    /// Record `path` (its row spelling `shown`) unless its content is binary
    /// ([`Passed::of`]).
    pub fn record(&mut self, path: &Path, shown: String) {
        if let Some(passed) = Passed::of(path, shown) {
            self.add(passed);
        }
    }

    /// Record a file judged by [`Passed::of`], by its known type, if any;
    /// a file already recorded is not counted again.
    pub fn add(&mut self, passed: Passed) {
        if self.passed.insert(passed.path, passed.known).is_some() {
            return;
        }
        if let Some(known) = passed.known {
            self.typed
                .entry(known.to_owned())
                .or_default()
                .record(passed.shown.clone());
        } else {
            self.untyped.record(passed.shown.clone());
        }
        self.files.record(passed.shown);
    }

    /// What a page discloses of the grammars it `read` ([`SourceTypes`]): a
    /// file of no known type, or of a type those grammars' names are of, may
    /// hold their source, so it warns; a file of another known type holds
    /// none, so it is a quiet count per type beside the grammars the query
    /// does not compile in. With no grammar read (`None`), every such file
    /// warns.
    pub fn disclosed(
        &self,
        reader: &Reader,
        read: Option<&SourceTypes>,
        mut skipped_grammars: Option<Value>,
    ) -> (Option<String>, Option<Value>) {
        if read.is_none() {
            return (unparsed_notice(reader, &self.files), skipped_grammars);
        }
        let mut named = self.untyped.clone();
        for (name, files) in &self.typed {
            if warns(Some(name.as_str()), read) {
                named.merge(files);
                continue;
            }
            let counts = skipped_grammars.get_or_insert_with(|| Value::Object(Default::default()));
            let known = counts[name.as_str()].as_u64().unwrap_or_default();
            counts[name.as_str()] = Value::from(known.saturating_add(u64::from(files.count)));
        }
        (unparsed_notice(reader, &named), skipped_grammars)
    }

    /// Every file the notice of [`Self::disclosed`] counts for the same
    /// `read`, in path order: one `unparsed` entry each, its type the reason
    /// when it has one. A file of another known type stays a count there.
    pub fn skipped(&self, read: Option<&SourceTypes>) -> Vec<SkippedEntry> {
        self.passed
            .iter()
            .filter(|(_, known)| warns(**known, read))
            .map(|(path, known)| {
                let entry = SkippedEntry::path(path.clone(), SkippedPathCause::Unparsed);
                match known {
                    Some(known) => entry.because(*known),
                    None => entry,
                }
            })
            .collect()
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

#[cfg(test)]
mod tests;
