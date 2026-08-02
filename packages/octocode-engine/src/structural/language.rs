use std::borrow::Cow;

use tree_sitter::Language as TSLanguage;

use crate::signatures::languages::LanguageEntry;

/// The stand-in identifier char(s) substituted for a `$`-sigil metavar so the
/// tree-sitter parser accepts the pattern as syntactically valid source.
/// `bare_word` differs from `primary` only for PHP/Bash — see its doc comment
/// — and is otherwise identical, so passing one `Expando` around instead of
/// two loose `char`s is a real invariant (they're always sourced together),
/// not just fewer function parameters.
#[derive(Clone, Copy)]
pub(super) struct Expando {
    /// Used everywhere a metavar is NOT at a bare-word position (see
    /// `is_bare_word_position`) — the common case.
    primary: char,
    /// Stand-in for a metavar landing at a *bare-word* position — a function
    /// name in PHP/Bash, where `$` (their `primary`, chosen so `$ARG`-shaped
    /// variable/argument patterns stay valid PHP/Bash syntax) is never
    /// legal: PHP/Bash function names are plain identifiers, never
    /// `$`-prefixed. Equal to `primary` for every other language (no
    /// behavioral difference).
    bare_word: char,
}

impl Expando {
    fn for_ext(ext: &str) -> Self {
        let primary = primary_expando_for_ext(ext);
        let bare_word = if primary == '$' { '_' } else { primary };
        Self { primary, bare_word }
    }

    /// Recognizes a metavar substituted with *either* char — see
    /// `meta_from_node`'s doc comment for why both must be checked.
    pub(super) fn matches_leading(self, c: char) -> bool {
        c == self.primary || c == self.bare_word
    }
}

/// A tree-sitter language wrapper. A single wrapper covers every grammar; the
/// only per-language knob is `expando`, the stand-in identifier char(s) used
/// while parsing a pattern in languages where `$` is not a valid identifier
/// character (Rust/Go/Python/C/...).
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
}

impl AgLanguage {
    pub(super) fn new(ext: &str, entry: &LanguageEntry) -> Self {
        Self {
            ts: entry.language.clone(),
            expando: Expando::for_ext(ext),
            php_wrap: ext == "php",
            class_wrap: ext == "cs",
        }
    }

    pub(super) fn tree_sitter_language(&self) -> TSLanguage {
        self.ts.clone()
    }

    pub(super) fn expando(&self) -> Expando {
        self.expando
    }

    pub(super) fn preprocess_pattern<'query>(&self, query: &'query str) -> Cow<'query, str> {
        let substituted = pre_process_pattern(self.expando, query);
        if self.php_wrap {
            Cow::Owned(format!("<?php {substituted}"))
        } else if self.class_wrap {
            Cow::Owned(format!("class __OctoWrap {{ {substituted} }}"))
        } else {
            substituted
        }
    }
}

/// The primary stand-in identifier char for `$` metavariables, per language.
/// Languages where `$` is a legal identifier char (JS/TS/Java/Bash/PHP) keep
/// `$`; the rest get a char the grammar accepts.
fn primary_expando_for_ext(ext: &str) -> char {
    match ext {
        // PHP variables require the `$` sigil (e.g. `$var`), so `$` is a valid
        // identifier character in tree-sitter-php. Patterns like `foo($ARG)` must
        // stay as-is for the PHP parser to accept them as a call with a variable arg.
        "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" | "java" | "sh" | "bash"
        | "zsh" | "php" => '$',
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => '\u{10000}',
        "html" | "htm" => 'z',
        "css" | "scss" | "less" => '_',
        // Erlang variables must start with an ASCII uppercase letter or `_`;
        // Elixir identifiers are ASCII-only (a leading non-ASCII byte breaks
        // its tokenizer); Zig identifiers are likewise ASCII-only. The
        // default µ (below) satisfies none of these, so every pattern
        // containing a metavar failed to parse/tokenize for all three — `_`
        // is valid in all of them and, being non-alphabetic, is unambiguous
        // with `is_capture_name`'s uppercase-only rest-of-name check.
        "erl" | "hrl" | "ex" | "exs" | "zig" => '_',
        "scala" | "sc" | "sbt" => '\u{00b5}',
        _ => '\u{00b5}',
    }
}

/// A metavar sits at a *bare-word* position when the text immediately before
/// it (ignoring whitespace) is empty (the metavar opens the pattern — Bash's
/// `$NAME() { ... }` function-definition shorthand) or the keyword
/// `function` (PHP's `function $NAME(...)`, Bash's alternate `function
/// $NAME { ... }` form). Only matters for languages whose `expando.primary`
/// is `$` (see `Expando::for_ext`); checked unconditionally for every
/// language for simplicity, since it's a no-op everywhere else.
fn is_bare_word_position(preceding: &[char]) -> bool {
    let trimmed_len = preceding
        .iter()
        .rposition(|c| !c.is_whitespace())
        .map_or(0, |i| i + 1);
    if trimmed_len == 0 {
        return true;
    }
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
    if is_bare_word_position(ret) {
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
