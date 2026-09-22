use crate::minify::comment_remover::remove_comments;

// ── JS/TS content-view minifier ───────────────────────────────────────────────

/// JS/TS content-view minifier: strip comments, collapse whitespace, and tighten
/// punctuation while **preserving every identifier, type, and per-statement line
/// structure**. This is the right trade for an LLM/agent content view — mangling
/// or AST-recodegen (the old OXC path) renamed/eliminated named bindings and
/// flattened the file to a single line, breaking symbol citation and file:line
/// mapping (the very evidence octocode exists to provide). Unlike a full parser
/// it needs no valid parse, so it degrades gracefully on partial/invalid source.
pub fn minify_javascript_core(content: &str) -> String {
    let s = remove_comments(content, &["c-style"]);
    // "c-style" already carries `regex: true` and the default quote/backtick
    // delimiters, so a single literal-range scan protects string, template,
    // and regex literals from the whitespace/punctuation passes below.
    let rules = crate::minify::comment_remover::rules_for("c-style");
    let s = super::collapse_whitespace(&s, rules.as_ref());
    let s = re_tighten_punct_js(&s, rules.as_ref());
    // Split back to lines, drop empty
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn re_tighten_punct_js(
    s: &str,
    rules: Option<&crate::minify::comment_remover::CommentRules>,
) -> String {
    let ranges = rules
        .map(|r| crate::minify::comment_remover::literal_ranges(s, r))
        .unwrap_or_default();
    let mut ri = 0usize;
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if let Some((_, end)) = super::in_literal_at(&ranges, &mut ri, i) {
            result.push_str(&s[i..end]);
            i = end;
            continue;
        }
        let b = bytes[i];
        if b == b' '
            && matches!(
                bytes.get(i + 1).copied(),
                Some(b'{' | b'}' | b'(' | b')' | b';' | b',' | b':')
            )
        {
            i += 1;
            continue;
        }
        if matches!(b, b'{' | b'}' | b'(' | b')' | b';' | b',') && bytes.get(i + 1) == Some(&b' ') {
            result.push(b as char);
            i += 2;
            continue;
        }
        i = super::copy_seq(s, i, &mut result);
    }
    result
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minify_javascript_core_strips_comments_keeps_identifiers_and_lines() {
        let src = "// header comment\nexport function resolveNativeBin(env: NodeEnv): string | null {\n  const explicitOverride = env.OCTOCODE_NATIVE_BIN?.trim(); // inline\n  /* block */\n  return explicitOverride;\n}\n";
        let out = minify_javascript_core(src);
        // Comments gone.
        assert!(!out.contains("header comment") && !out.contains("inline") && !out.contains("block"));
        // Identifiers + types preserved (citation/comprehension).
        for ident in ["resolveNativeBin", "explicitOverride", "OCTOCODE_NATIVE_BIN", "NodeEnv"] {
            assert!(out.contains(ident), "identifier {ident} must survive minification");
        }
        // Line structure preserved (NOT collapsed to a single line).
        assert!(out.lines().count() >= 3, "per-statement lines must be retained, got:\n{out}");
    }

    #[test]
    fn minify_javascript_core_never_panics_on_malformed_input() {
        for src in [
            "",
            "\u{0}\u{0}\u{0}",
            "function(){",
            "}}}}}}}}}}",
            "const x = /[/;",
            "import type type type from from;",
            "\u{feff}\u{202e}reversed",
        ] {
            let _ = minify_javascript_core(src);
        }
    }
}
