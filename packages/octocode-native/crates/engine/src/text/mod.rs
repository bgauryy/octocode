pub mod case_fold;
pub(crate) mod file_extension;
pub(crate) mod source;
pub mod test_paths;
pub(crate) mod utf8_offsets;

/// The one extension helper, JS/TS extension set, line index, leading-BOM
/// rule, bounded source read, regular-file open, and binary sniff.
pub use file_extension::{JS_EXTENSIONS, JS_TS_EXTENSIONS, extension_of};
pub use source::{
    BinaryVerdict, Decoded, Decoding, ReadError, Refusal, Source, admit, decode, has_utf16_bom,
    open_regular, read_source, read_source_from, sniff, sniff_complete,
};
pub use utf8_offsets::{LineIndex, bom_len, line_at, line_starts};

/// `path` as a string with `/` separators on every platform: the one form
/// a path takes in rows, keys and globs.
pub fn slash_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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
        assert_eq!(
            super::slash_path(std::path::Path::new("a\\b\\c.rs")),
            "a/b/c.rs"
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
