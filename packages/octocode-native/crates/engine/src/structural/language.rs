use std::borrow::Cow;

use tree_sitter::Language;

use crate::signatures::languages::LanguageEntry;

/// Name of the synthetic class (C#) or function (C family) a pattern is
/// parsed inside when it needs member or statement context. Real code
/// essentially never declares this name, so `effective_pattern_root` can
/// unwrap the wrapper by name without swallowing a user's own declaration.
pub(super) const WRAP_MARKER: &str = "__OctoWrap";

/// `statement` as the sole statement of the synthetic C-family function.
pub(super) fn wrap_in_function_body(statement: &str) -> String {
    format!("void {WRAP_MARKER}(void) {{ {statement} }}")
}

/// Stand-in identifier character substituted for a `$`-sigil metavar so the
/// selected Tree-sitter grammar accepts the pattern as valid source.
#[derive(Clone, Copy)]
pub(super) struct Expando(char);

impl Expando {
    pub(super) fn matches_leading(self, c: char) -> bool {
        c == self.0
    }

    /// `text` with this sigil spelled `$` again, as the caller wrote it.
    pub(super) fn restore(self, text: &str) -> String {
        text.replace(self.0, "$")
    }
}

/// Shared tree-sitter wrapper with grammar-specific identifier substitutions
/// and parsing contexts for patterns that are not complete source documents.
#[derive(Clone)]
pub(super) struct AgLanguage {
    ts: Language,
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
    /// A C-family grammar (C, C++, CUDA). Its statements are only valid
    /// inside a function body. At top level the grammars reject some outright
    /// and reshape others through error recovery: a CUDA launch
    /// `k<<<g, b>>>(x);` becomes a `parameter_pack_expansion` with a MISSING
    /// `...`, which matches no real launch. A pattern that does not parse
    /// cleanly alone is reparsed as the sole statement of a synthetic
    /// function. Its `#define` bodies are re-parsed as source too (see
    /// `macro_bodies`).
    c_family: bool,
    /// Optional terminator context for fragments whose grammar otherwise treats
    /// a bare call/declaration as an error or a selector. The compiler accepts
    /// the contextual parse only when it produces this exact node kind.
    terminated_fragment_kind: Option<&'static str>,
}

impl AgLanguage {
    pub(super) fn new(entry: &LanguageEntry) -> Self {
        Self {
            ts: entry.language.clone(),
            expando: Expando(entry.pattern_syntax.expando),
            class_wrap: entry.pattern_syntax.member_context,
            c_family: entry.pattern_syntax.function_context,
            terminated_fragment_kind: entry.pattern_syntax.terminated_fragment_kind,
        }
    }

    pub(super) fn tree_sitter_language(&self) -> Language {
        self.ts.clone()
    }

    pub(super) fn expando(&self) -> Expando {
        self.expando
    }

    /// Whether patterns are first parsed inside a synthetic class body.
    pub(super) fn class_wraps(&self) -> bool {
        self.class_wrap
    }

    /// Whether this is a C-family grammar (see `c_family`).
    pub(super) fn is_c_family(&self) -> bool {
        self.c_family
    }

    pub(super) fn terminated_fragment_kind(&self) -> Option<&'static str> {
        self.terminated_fragment_kind
    }

    pub(super) fn preprocess_metavariables<'query>(&self, query: &'query str) -> Cow<'query, str> {
        pre_process_pattern(self.expando, query)
    }

    pub(super) fn preprocess_pattern<'query>(&self, query: &'query str) -> Cow<'query, str> {
        match self.preprocess_metavariables(query) {
            Cow::Borrowed(text) => self.wrap_class(text),
            Cow::Owned(text) => Cow::Owned(self.wrap_class(&text).into_owned()),
        }
    }

    /// `text` inside the synthetic class, when this grammar parses patterns
    /// in one (see `class_wrap`).
    fn wrap_class<'text>(&self, text: &'text str) -> Cow<'text, str> {
        if self.class_wrap {
            Cow::Owned(format!("class {WRAP_MARKER} {{ {text} }}"))
        } else {
            Cow::Borrowed(text)
        }
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
