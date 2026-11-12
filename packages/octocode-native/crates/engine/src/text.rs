pub mod case_fold;
pub(crate) mod file_extension;
mod ident;
mod source;
pub use source::analysis;
pub mod support_paths;
pub mod test_paths;
pub(crate) mod utf8_offsets;
mod words;

/// The one extension helper, JS/TS extension set, identifier-boundary rule,
/// line index, leading-BOM rule, bounded source read, regular-file open, and
/// binary sniff.
pub use file_extension::{JS_EXTENSIONS, JS_TS_EXTENSIONS, basename, extension_of};
pub use ident::{is_ident_char, is_whole_word, whole_word_starts, word_pattern};
pub use source::{
    BinaryVerdict, Decoded, Decoding, ReadError, Refusal, Source, admit, decode, has_utf16_bom,
    SNIFF_BYTES, interior_offset, open_regular, read_regular, read_source, read_source_from,
    sniff, sniff_complete,
};
pub use utf8_offsets::{LineIndex, bom_len, line_at, line_containing, line_starts};
pub use words::identifier_words;

/// `count` and its noun in the right number: `1 line`, `2 lines`, `0 lines`.
pub fn counted<N>(count: N, singular: &str, plural: &str) -> String
where
    N: std::fmt::Display + PartialEq + From<u8>,
{
    let noun = if count == N::from(1) {
        singular
    } else {
        plural
    };
    format!("{count} {noun}")
}

/// `path` as a string with `/` separators on every platform: the one form
/// a path takes in rows, keys and globs. Only Windows rewrites `\`: on Unix
/// it is a legal file-name byte, and rewriting it would name another path.
pub fn slash_path(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.into_owned()
    }
}

/// `path` with `.` and `..` folded lexically, without touching the file
/// system: the one fold. A `..` above the first component is dropped.
#[must_use]
pub fn lexical(path: &std::path::Path) -> std::path::PathBuf {
    let mut folded = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                folded.pop();
            }
            other => folded.push(other),
        }
    }
    folded
}

#[cfg(test)]
mod slash_path_tests {
    #[test]
    fn a_path_reads_with_forward_slashes() {
        assert_eq!(super::slash_path(std::path::Path::new("a/b.rs")), "a/b.rs");
        #[cfg(windows)]
        assert_eq!(
            super::slash_path(std::path::Path::new("a\\b\\c.rs")),
            "a/b/c.rs"
        );
        #[cfg(not(windows))]
        assert_eq!(
            super::slash_path(std::path::Path::new("a\\b\\c.rs")),
            "a\\b\\c.rs"
        );
        assert_eq!(super::slash_path(std::path::Path::new("")), "");
    }

    #[test]
    fn a_path_folds_dot_components_without_the_file_system() {
        for (path, folded) in [
            ("a/./b/../c", "a/c"),
            ("./a/b/..", "a"),
            ("../a", "a"),
            ("../../a/../b", "b"),
            ("/../a", "/a"),
            ("/a/b/../../..", "/"),
            ("/missing/./x/../y", "/missing/y"),
            ("", ""),
        ] {
            assert_eq!(
                super::lexical(std::path::Path::new(path)),
                std::path::PathBuf::from(folded),
                "{path}"
            );
        }
    }
}
