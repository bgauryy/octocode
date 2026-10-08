use std::borrow::Cow;

use tree_sitter::Language as TSLanguage;

use crate::signatures::languages::LanguageEntry;
use crate::text::file_extension::JS_TS_EXTENSIONS;

/// Stand-in identifier character substituted for a `$`-sigil metavar so the
/// selected Tree-sitter grammar accepts the pattern as valid source.
#[derive(Clone, Copy)]
pub(super) struct Expando(char);

impl Expando {
    fn for_ext(ext: &str) -> Self {
        Self(primary_expando_for_ext(ext))
    }

    pub(super) fn matches_leading(self, c: char) -> bool {
        c == self.0
    }
}

/// Shared tree-sitter wrapper with grammar-specific identifier substitutions
/// and parsing contexts for patterns that are not complete source documents.
#[derive(Clone)]
pub(super) struct AgLanguage {
    ts: TSLanguage,
    expando: Expando,
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
            class_wrap: ext == "cs",
            terminated_fragment_kind: terminated_fragment_kind_for_ext(ext),
        }
    }

    pub(super) fn tree_sitter_language(&self) -> TSLanguage {
        self.ts.clone()
    }

    pub(super) fn expando(&self) -> Expando {
        self.expando
    }

    /// Whether patterns are first parsed inside a synthetic class body.
    pub(super) fn class_wraps(&self) -> bool {
        self.class_wrap
    }

    pub(super) fn terminated_fragment_kind(&self) -> Option<&'static str> {
        self.terminated_fragment_kind
    }

    pub(super) fn preprocess_rewrite_pattern<'query>(
        &self,
        query: &'query str,
    ) -> Cow<'query, str> {
        pre_process_pattern(self.expando, query)
    }

    pub(super) fn preprocess_pattern<'query>(&self, query: &'query str) -> Cow<'query, str> {
        let substituted = self.preprocess_rewrite_pattern(query);
        if self.class_wrap {
            Cow::Owned(format!("class __OctoWrap {{ {substituted} }}"))
        } else {
            substituted
        }
    }
}

/// Node kind a bare call fragment (`foo($A)`) must parse to once a `;`
/// terminator supplies statement context. Without it, C/C++ reject the
/// fragment and Go parses it as a type conversion, so calls silently miss.
fn terminated_fragment_kind_for_ext(ext: &str) -> Option<&'static str> {
    match ext {
        "java" => Some("method_invocation"),
        "rs" | "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "go" => {
            Some("call_expression")
        }
        _ => None,
    }
}

/// Primary stand-in identifier character for `$` metavariables, per language.
/// Languages where `$` is a legal identifier character keep it; C-family
/// grammars use an astral Unicode letter to avoid collisions; the ASCII-only
/// Assembly grammar uses `Q`; other grammars use `µ`.
pub(super) fn primary_expando_for_ext(ext: &str) -> char {
    match ext {
        ext if ext == "java" || JS_TS_EXTENSIONS.contains(&ext) => '$',
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "cu" | "cuh" => '\u{10000}',
        "asm" | "assembly" | "s" => 'Q',
        _ => '\u{00b5}',
    }
}

/// Rewrites the `$` sigil of capturing/anonymous-multiple metavars to the
/// language's expando character. Literal non-metavariable `$` is preserved.
fn pre_process_pattern(expando: Expando, query: &str) -> Cow<'_, str> {
    let mut ret = String::with_capacity(query.len());
    let mut dollar_count = 0;
    for c in query.chars() {
        if c == '$' {
            dollar_count += 1;
            continue;
        }
        let replace = matches!(c, 'A'..='Z' | '_') || dollar_count == 3;
        let sigil = if replace && dollar_count > 0 {
            expando.0
        } else {
            '$'
        };
        ret.extend(std::iter::repeat_n(sigil, dollar_count));
        dollar_count = 0;
        ret.push(c);
    }
    let sigil = if dollar_count == 3 { expando.0 } else { '$' };
    ret.extend(std::iter::repeat_n(sigil, dollar_count));
    Cow::Owned(ret)
}
