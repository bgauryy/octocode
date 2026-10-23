//! Comment rules per comment group, and the quote delimiters a group uses.

#[derive(Clone)]
pub struct BlockRule {
    pub start: &'static str,
    pub end: &'static str,
    pub nested: bool,
}

#[derive(Clone)]
pub struct LineRule {
    pub token: &'static str,
    pub require_boundary: bool, // false → always strip (e.g. SQL --)
    pub preserve_shebang: bool,
}

#[derive(Clone, Default)]
pub struct CommentRules {
    pub block: &'static [BlockRule],
    pub line: &'static [LineRule],
    pub regex: bool,
    pub powershell_here_strings: bool,
    pub quote_delimiters: &'static [&'static str],
}

const NO_RULES: CommentRules = CommentRules {
    block: &[],
    line: &[],
    regex: false,
    powershell_here_strings: false,
    quote_delimiters: &[],
};

/// A boundary-delimited line comment that does not protect a shebang.
const fn line(token: &'static str) -> LineRule {
    LineRule {
        token,
        require_boundary: true,
        preserve_shebang: false,
    }
}

/// A non-nesting block comment.
const fn block(start: &'static str, end: &'static str) -> BlockRule {
    BlockRule {
        start,
        end,
        nested: false,
    }
}

const HASH_LINE: LineRule = LineRule {
    token: "#",
    require_boundary: true,
    preserve_shebang: true,
};

/// Comment rules per comment group name (the names a [`FileTypeConfig`] lists).
///
/// [`FileTypeConfig`]: crate::minify::config::FileTypeConfig
pub(super) static COMMENT_GROUPS: &[(&str, CommentRules)] = &[
    (
        "c-style",
        CommentRules {
            block: &[block("/*", "*/")],
            line: &[line("//")],
            regex: true,
            ..NO_RULES
        },
    ),
    (
        "hash",
        CommentRules {
            line: &[HASH_LINE],
            ..NO_RULES
        },
    ),
    (
        "html",
        CommentRules {
            block: &[block("<!--", "-->")],
            ..NO_RULES
        },
    ),
    (
        "sql",
        CommentRules {
            block: &[block("/*", "*/")],
            line: &[LineRule {
                token: "--",
                require_boundary: false,
                preserve_shebang: false,
            }],
            ..NO_RULES
        },
    ),
    (
        "haskell",
        CommentRules {
            block: &[block("{-", "-}")],
            line: &[line("--")],
            ..NO_RULES
        },
    ),
    (
        "semicolon",
        CommentRules {
            line: &[line(";")],
            ..NO_RULES
        },
    ),
    // Clojure/ClojureScript: `'` and `` ` `` are quote/syntax-quote prefixes,
    // never string delimiters (strings are `"..."` only) — unlike the
    // `DEFAULT_QUOTE_DELIMITERS` fallback, which would treat a stray `'foo`
    // as an unterminated string and swallow everything after it, including
    // real string literals, so their whitespace never gets protected from
    // the aggressive strategy's collapse/tighten passes.
    (
        "clojure",
        CommentRules {
            line: &[line(";")],
            quote_delimiters: &["\"\"\"", "\""],
            ..NO_RULES
        },
    ),
    (
        "wasm-text",
        CommentRules {
            block: &[block("(;", ";)")],
            line: &[line(";;")],
            ..NO_RULES
        },
    ),
    (
        "percent",
        CommentRules {
            line: &[line("%")],
            ..NO_RULES
        },
    ),
    (
        "haml",
        CommentRules {
            line: &[line("-#")],
            ..NO_RULES
        },
    ),
    (
        "slim",
        CommentRules {
            line: &[line("/")],
            ..NO_RULES
        },
    ),
    (
        "powershell",
        CommentRules {
            block: &[block("<#", "#>")],
            line: &[HASH_LINE],
            powershell_here_strings: true,
            ..NO_RULES
        },
    ),
    (
        "bang",
        CommentRules {
            line: &[line("!")],
            ..NO_RULES
        },
    ),
    (
        "apostrophe",
        CommentRules {
            line: &[line("'")],
            quote_delimiters: &["\"\"\"", "\"", "`"],
            ..NO_RULES
        },
    ),
    (
        "double-dash",
        CommentRules {
            line: &[line("--")],
            ..NO_RULES
        },
    ),
    (
        "fsharp-block",
        CommentRules {
            block: &[BlockRule {
                start: "(*",
                end: "*)",
                nested: true,
            }],
            ..NO_RULES
        },
    ),
    (
        "pascal",
        CommentRules {
            block: &[
                BlockRule {
                    start: "(*",
                    end: "*)",
                    nested: true,
                },
                block("{", "}"),
            ],
            line: &[line("//")],
            quote_delimiters: &["'", "\""],
            ..NO_RULES
        },
    ),
    (
        "template",
        CommentRules {
            block: &[
                block("{{!--", "--}}"),
                block("{{!", "}}"),
                block("<%#", "%>"),
                block("{#", "#}"),
            ],
            ..NO_RULES
        },
    ),
];

/// The [`CommentRules`] of the named comment group.
pub fn rules_for(group: &str) -> Option<&'static CommentRules> {
    COMMENT_GROUPS
        .iter()
        .find(|(name, _)| *name == group)
        .map(|(_, rules)| rules)
}

const DEFAULT_QUOTE_DELIMITERS: &[&str] = &["\"\"\"", "'''", "\"", "'", "`"];

pub(super) fn effective_delimiters(rules: &CommentRules) -> &[&'static str] {
    if rules.quote_delimiters.is_empty() {
        DEFAULT_QUOTE_DELIMITERS
    } else {
        rules.quote_delimiters
    }
}
