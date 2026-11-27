//! What one Rust text spells about the modules it could mount, read in a single
//! pass: the identifiers that follow a `mod` head and the `.rs` file names it
//! mentions. Asking [`super::rust_source::may_declare_module`] and
//! [`super::rust_source::word_at`] per child rescans the whole text each time;
//! a parent asked about many children is scanned here once instead, and a
//! lookup answers as the scans do.
use super::rust_source::{may_declare_module, word_at};
use std::collections::{HashMap, HashSet};

const RUST_SUFFIX: &str = ".rs";

/// Texts the head index must answer as the scans do, shared with the mount
/// tests that compare scans with parses over them.
#[cfg(test)]
pub(super) const TEXTS: [&str; 9] = [
    "mod deep;\nmod other;",
    "mod /* c */ deep {}\n#[path = \"a/x.rs\"] mod x;",
    "// mod\nlet deeper = 1;",
    "mod // d\n    r#deep;\n",
    "submod deep; fn deep() {}",
    "pub(crate) mod\n\tdeep;",
    "let s = \"b.rs\"; a.b.rs; ab.rs2; x.rs",
    "mod é; mod ünï; // mod tail\ndeep",
    "",
];

/// The identifiers and file names of one text, indexed.
pub(super) struct ModuleHeads {
    /// Identifier → the last byte it starts at as the name of a `mod` head.
    modules: HashMap<Box<str>, usize>,
    /// Every `<identifier>.rs` the text spells as a word.
    files: HashSet<Box<str>>,
}

fn is_word_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric()
}

/// Whether `name` is one identifier run: the only spelling an index answers.
fn is_identifier(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(is_word_byte)
}

/// The offsets of `needle` in `text`, in order.
fn offsets(text: &[u8], needle: &[u8]) -> Vec<usize> {
    memchr::memmem::find_iter(text, needle).collect()
}

/// The text with the offsets of its newlines and `//`, to read what precedes
/// an identifier without rescanning its line.
struct HeadReader<'a> {
    content: &'a str,
    newlines: Vec<usize>,
    comments: Vec<usize>,
}

impl<'a> HeadReader<'a> {
    fn new(content: &'a str) -> Self {
        Self {
            content,
            newlines: memchr::memchr_iter(b'\n', content.as_bytes()).collect(),
            comments: offsets(content.as_bytes(), b"//"),
        }
    }

    /// Whether the identifier starting at `at` is named by a `mod` head, as
    /// [`super::rust_source::may_declare_module`] reads the text before it.
    fn follows_module_keyword(&self, at: usize) -> bool {
        let Some(before) = self.content.get(..at) else {
            return false;
        };
        let before = before.strip_suffix("r#").unwrap_or(before).trim_end();
        let end = before.len();
        before.ends_with("*/")
            || self.comment_in_line_ending_at(end)
            || before
                .strip_suffix("mod")
                .is_some_and(|head| !head.as_bytes().last().is_some_and(|b| is_word_byte(*b)))
    }

    /// Whether the line that ends at `end` holds a `//` before it.
    fn comment_in_line_ending_at(&self, end: usize) -> bool {
        let start = self.newlines.partition_point(|newline| *newline < end);
        let line_start = start
            .checked_sub(1)
            .map_or(0, |last| self.newlines[last] + 1);
        let first = self
            .comments
            .partition_point(|comment| *comment < line_start);
        self.comments
            .get(first)
            .is_some_and(|comment| comment + "//".len() <= end)
    }
}

impl ModuleHeads {
    pub(super) fn of(content: &str) -> Self {
        let reader = HeadReader::new(content);
        let bytes = content.as_bytes();
        let mut modules = HashMap::new();
        let mut files = HashSet::new();
        let mut start = 0;
        while start < bytes.len() {
            if !is_word_byte(bytes[start]) {
                start += 1;
                continue;
            }
            let end = bytes[start..]
                .iter()
                .position(|byte| !is_word_byte(*byte))
                .map_or(bytes.len(), |len| start + len);
            let Some(word) = content.get(start..end) else {
                start = end;
                continue;
            };
            if reader.follows_module_keyword(start) {
                modules.insert(Box::from(word), start);
            }
            if bytes[end..].starts_with(RUST_SUFFIX.as_bytes())
                && !bytes
                    .get(end + RUST_SUFFIX.len())
                    .is_some_and(|byte| is_word_byte(*byte))
            {
                files.insert(format!("{word}{RUST_SUFFIX}").into());
            }
            start = end;
        }
        Self { modules, files }
    }

    /// [`may_declare_module`] of `content` (the text this was built from).
    pub(super) fn declares_module(&self, content: &str, name: &str, from: usize) -> bool {
        if is_identifier(name) {
            self.modules.get(name).is_some_and(|last| *last >= from)
        } else {
            may_declare_module(content, name, from)
        }
    }

    /// Whether `content` (the text this was built from) spells `file` as a
    /// word, as [`word_at`] finds it.
    pub(super) fn names_file(&self, content: &str, file: &str) -> bool {
        let stem = file.strip_suffix(RUST_SUFFIX);
        if stem.is_some_and(is_identifier) {
            self.files.contains(file)
        } else {
            word_at(content, file, 0).is_some()
        }
    }
}

#[cfg(test)]
mod tests;
