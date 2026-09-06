use std::borrow::Cow;

use tree_sitter::Language as TSLanguage;

use crate::signatures::languages::LanguageEntry;

/// The stand-in identifier char(s) substituted for a `$`-sigil metavar so the
/// tree-sitter parser accepts the pattern as syntactically valid source.
/// PHP function names need a different stand-in from PHP variables.
#[derive(Clone, Copy)]
pub(super) struct Expando {
    /// Used everywhere a metavar is NOT at a bare-word position (see
    /// `is_bare_word_position`) — the common case.
    primary: char,
    /// Stand-in after PHP's `function` keyword, where `$` is not legal.
    /// Equal to `primary` for every other language.
    bare_word: char,
}

impl Expando {
    fn for_ext(ext: &str) -> Self {
        let primary = primary_expando_for_ext(ext);
        let bare_word = if ext == "php" { '_' } else { primary };
        Self { primary, bare_word }
    }

    /// Recognizes a metavar substituted with *either* char — see
    /// `meta_from_node`'s doc comment for why both must be checked.
    pub(super) fn matches_leading(self, c: char) -> bool {
        c == self.primary || c == self.bare_word
    }
}

/// Shared tree-sitter wrapper with grammar-specific identifier substitutions
/// and parsing contexts for patterns that are not complete source documents.
#[derive(Clone)]
pub(super) struct AgLanguage {
    ts: TSLanguage,
    expando: Expando,
    /// PHP source is a text/HTML host with `<?php ... ?>` islands of real PHP
    /// code — anything outside those tags parses as opaque `text`, not
    /// statements. A bare pattern like `$x = 5;` (no `<?php` tag) parsed on
    /// its own is swallowed whole into one `text` node, which never appears
    /// as a candidate when walking a real document — every PHP pattern
    /// silently matched nothing. `true` for `.php` only.
    php_wrap: bool,
    /// C# has no top-level method/member syntax: a modifier like `public` is
    /// only valid inside a `class`/`struct`/`interface` body, so a bare
    /// pattern like `public int $NAME(...) { ... }` parsed standalone lands
    /// on the wrong top-level construct (`global_statement` /
    /// `local_function_statement`, a C# 9+ top-level-statements artifact —
    /// never a real candidate kind for a class method) instead of
    /// `method_declaration`. Wrapping in a throwaway class gives the parser
    /// real member context. `true` for `.cs` only.
    class_wrap: bool,
    /// Optional terminator context for fragments whose grammar otherwise treats
    /// a bare call/declaration as an error or a selector. The compiler accepts
    /// the contextual parse only when it produces this exact node kind.
    terminated_fragment_kind: Option<&'static str>,
}

impl AgLanguage {
    pub(super) fn new(ext: &str, entry: &LanguageEntry) -> Self {
        Self {
            ts: entry.language.clone(),
            expando: Expando::for_ext(ext),
            php_wrap: ext == "php",
            class_wrap: ext == "cs",
            terminated_fragment_kind: match ext {
                "java" => Some("method_invocation"),
                "css" | "scss" => Some("declaration"),
                _ => None,
            },
        }
    }

    pub(super) fn tree_sitter_language(&self) -> TSLanguage {
        self.ts.clone()
    }

    pub(super) fn expando(&self) -> Expando {
        self.expando
    }

    pub(super) fn terminated_fragment_kind(&self) -> Option<&'static str> {
        self.terminated_fragment_kind
    }

    pub(super) fn preprocess_pattern<'query>(&self, query: &'query str) -> Cow<'query, str> {
        let substituted = pre_process_pattern(self.expando, query);
        if self.php_wrap
            && !substituted
                .trim_start()
                .get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("<?php"))
            && !substituted.trim_start().starts_with("<?=")
        {
            Cow::Owned(format!("<?php {substituted}"))
        } else if self.class_wrap {
            Cow::Owned(format!("class __OctoWrap {{ {substituted} }}"))
        } else {
            substituted
        }
    }
}

/// The primary stand-in identifier char for `$` metavariables, per language.
/// Languages where `$` is a legal identifier char (JS/TS/Java/PHP) keep
/// `$`; the rest get a char the grammar accepts.
fn primary_expando_for_ext(ext: &str) -> char {
    match ext {
        // PHP variables require the `$` sigil (e.g. `$var`), so `$` is a valid
        // identifier character in tree-sitter-php. Patterns like `foo($ARG)` must
        // stay as-is for the PHP parser to accept them as a call with a variable arg.
        "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" | "java" | "php" => '$',
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => '\u{10000}',
        "html" | "htm" => 'z',
        "css" | "scss" => '_',
        // Zig requires an ASCII placeholder; `_` is unambiguous
        // with the uppercase-only capture-name syntax.
        "zig" => '_',
        // This SQL grammar rejects the default non-ASCII placeholder.
        "sql" => '_',
        _ => '\u{00b5}',
    }
}

/// PHP's `function $NAME(...)` pattern needs a plain function-name identifier.
fn is_bare_word_position(preceding: &[char]) -> bool {
    let trimmed_len = preceding
        .iter()
        .rposition(|c| !c.is_whitespace())
        .map_or(0, |i| i + 1);
    const KEYWORD: &str = "function";
    if trimmed_len < KEYWORD.len() {
        return false;
    }
    let tail: String = preceding[trimmed_len - KEYWORD.len()..trimmed_len]
        .iter()
        .collect();
    if tail != KEYWORD {
        return false;
    }
    // Whole-word match only — reject a longer identifier that merely ends in
    // "function" (e.g. a hypothetical `myfunction`).
    let before_keyword = trimmed_len - KEYWORD.len();
    before_keyword == 0
        || !preceding[before_keyword - 1].is_alphanumeric() && preceding[before_keyword - 1] != '_'
}

/// The sigil to substitute for a run of `dollar_count` consecutive `$`
/// immediately followed by an identifier-starting char (or already known to
/// be a `$$$` multi-capture) — `'$'` unchanged if it's not actually a
/// metavar (a single non-uppercase-leading `$`, e.g. a literal PHP `$var`),
/// otherwise `expando.bare_word` or `expando.primary` depending on `ret`'s
/// trailing context. Shared by both the in-loop substitution and the
/// pattern's trailing `$` run so the two can't drift.
fn sigil_for(ret: &[char], dollar_count: usize, expando: Expando) -> char {
    if dollar_count == 0 {
        return '$';
    }
    if expando.bare_word != expando.primary && is_bare_word_position(ret) {
        expando.bare_word
    } else {
        expando.primary
    }
}

/// Rewrites the `$` sigil of capturing/anonymous-multiple metavars to the
/// language's expando char so the tree-sitter parser accepts the pattern.
/// Literal `$` (e.g. a non-metavar `$` in the source) is preserved. A metavar
/// at a bare-word position (see `is_bare_word_position`) uses
/// `expando.bare_word` instead of `expando.primary`.
fn pre_process_pattern(expando: Expando, query: &str) -> Cow<'_, str> {
    let mut ret: Vec<char> = Vec::with_capacity(query.len());
    let mut dollar_count = 0;
    for c in query.chars() {
        if c == '$' {
            dollar_count += 1;
            continue;
        }
        let need_replace = matches!(c, 'A'..='Z' | '_') || dollar_count == 3;
        let sigil = if need_replace {
            sigil_for(&ret, dollar_count, expando)
        } else {
            '$'
        };
        ret.extend(std::iter::repeat_n(sigil, dollar_count));
        dollar_count = 0;
        ret.push(c);
    }
    let sigil = if dollar_count == 3 {
        sigil_for(&ret, dollar_count, expando)
    } else {
        '$'
    };
    ret.extend(std::iter::repeat_n(sigil, dollar_count));
    Cow::Owned(ret.into_iter().collect())
}
