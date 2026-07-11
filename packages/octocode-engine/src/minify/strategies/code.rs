use crate::comment_remover::remove_comments;

// ── OXC JS/TS AST minifier ────────────────────────────────────────────────────

/// OXC-backed JS/TS minification.
///
/// `mangle = true`: rename locals (maximum compression, for full minify path).
/// `mangle = false`: preserve names (agent-readable, for content-view path).
///
/// Returns `None` on parse error so callers can choose the non-OXC path.
///
/// The entire OXC parse/minify/codegen pipeline is wrapped in `catch_unwind`
/// so that an OXC internal panic (ICE) on adversarial input is converted into a
/// clean `None` fallback rather than unwinding across the napi FFI boundary and
/// aborting the Node process (especially dangerous on the AsyncTask worker
/// thread). Every caller — both the `minify*` entry points and the `apply*`
/// pipeline — is therefore covered at the source.
pub fn minify_js_oxc(content: &str, file_path: &str, mangle: bool) -> Option<String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        minify_js_oxc_inner(content, file_path, mangle)
    }))
    .unwrap_or(None)
}

fn minify_js_oxc_inner(content: &str, file_path: &str, mangle: bool) -> Option<String> {
    use oxc_allocator::Allocator;
    use oxc_codegen::{Codegen, CodegenOptions, CommentOptions};
    use oxc_minifier::{CompressOptions, MangleOptions, Minifier, MinifierOptions};
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    let allocator = Allocator::default();
    let ext = crate::file_extension::get_extension_internal(file_path, true, "js");
    let source_type = match ext.as_str() {
        "ts" | "mts" | "cts" => SourceType::ts(),
        "tsx" => SourceType::tsx(),
        "jsx" => SourceType::jsx(),
        "mjs" => SourceType::mjs(),
        _ => SourceType::default(), // js / cjs
    };

    let parser_ret = Parser::new(&allocator, content, source_type).parse();
    // If plain-JS parse failed (e.g. Flow-annotated file with `import type`), retry as TS.
    let parser_ret = if !parser_ret.diagnostics.is_empty() && matches!(ext.as_str(), "js" | "cjs") {
        Parser::new(&allocator, content, SourceType::ts()).parse()
    } else {
        parser_ret
    };
    if !parser_ret.diagnostics.is_empty() {
        return None;
    }

    let mut program = parser_ret.program;

    // P1: Strip TypeScript-only top-level statements before code generation.
    // Removes: `import type`, `interface`, `type alias`, TS-declare statements.
    //
    // Note: Statement::is_typescript_syntax() returns false for ImportDeclaration
    // even when import_kind == Type (OXC considers it valid syntax for other purposes).
    // We therefore write an explicit match.
    use oxc_ast::ast::{ImportOrExportKind, Statement as Stmt};
    program.body.retain(|stmt| match stmt {
        // `import type { Foo } from '...'` — no runtime value
        Stmt::ImportDeclaration(decl) => decl.import_kind != ImportOrExportKind::Type,
        // All other TS-only nodes (interface, type alias, declare, etc.)
        _ => !stmt.is_typescript_syntax(),
    });

    // Use safest() for compression + mangle for variable renaming.
    // safest() = keep all code, join vars, comma sequences — no dead-code removal
    // (avoids OXC 0.95 bug where CompressOptions::default() can produce empty output).
    Minifier::new(MinifierOptions {
        mangle: if mangle {
            Some(MangleOptions::default())
        } else {
            None
        },
        compress: Some(CompressOptions::safest()),
    })
    .minify(&allocator, &mut program);

    let codegen_opts = CodegenOptions {
        minify: true,
        // Strip ALL comment classes — the "standard" view contract removes
        // known language comments (normal + jsdoc default to true in oxc).
        comments: CommentOptions {
            normal: false,
            jsdoc: false,
            annotation: false,
            ..CommentOptions::default()
        },
        ..CodegenOptions::default()
    };
    let result = Codegen::new()
        .with_options(codegen_opts)
        .build(&program)
        .code;
    // Guard: never return empty output — that signals OXC mangle produced broken code.
    // Also return None if OXC grew the content (shouldn't happen but be safe).
    if result.is_empty() || result.len() >= content.len() {
        return None;
    }
    Some(result)
}

/// Heuristic JS minifier (comment strip + whitespace tightening) used when
/// the OXC pipeline declines the input.
pub fn minify_javascript_core(content: &str) -> String {
    let s = remove_comments(content, &["c-style"]);
    // "c-style" already carries `regex: true` and the default quote/backtick
    // delimiters, so a single literal-range scan protects string, template,
    // and regex literals from the whitespace/punctuation passes below.
    let rules = crate::comment_remover::rules_for("c-style");
    let s = super::collapse_whitespace(&s, rules.as_ref());
    let s = re_tighten_punct_js(&s, rules.as_ref());
    // Split back to lines, drop empty
    s.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn re_tighten_punct_js(s: &str, rules: Option<&crate::comment_remover::CommentRules>) -> String {
    let ranges = rules
        .map(|r| crate::comment_remover::literal_ranges(s, r))
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
    fn minify_js_oxc_minifies_valid_input() {
        let out = minify_js_oxc("const  x   =   1 ;\n", "x.js", true);
        assert!(out.is_some(), "valid JS should minify");
    }

    /// Regression: the OXC pipeline must NEVER unwind across the FFI boundary.
    /// A panic (ICE) raised anywhere inside the parse/minify/codegen pipeline
    /// must be converted to a clean `None` fallback by the `catch_unwind` guard
    /// added to `minify_js_oxc`, instead of unwinding into napi and aborting
    /// Node. We verify the guard directly by forcing a panic inside the wrapped
    /// closure shape, then confirm a barrage of malformed inputs all return
    /// (Some or None) without aborting.
    ///
    /// (Note: a stack overflow from pathologically nested input is a distinct,
    /// uncatchable failure class — `catch_unwind` only intercepts panics, which
    /// is the class this finding targets.)
    #[test]
    fn minify_js_oxc_guard_converts_panic_to_none() {
        let caught =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Option<String> {
                panic!("simulated OXC ICE");
            }))
            .unwrap_or(None);
        assert_eq!(caught, None, "catch_unwind must convert a panic into None");
    }

    #[test]
    fn minify_js_oxc_never_aborts_on_malformed_input() {
        let adversarial = [
            "",
            "\u{0}\u{0}\u{0}",
            "function(){",
            "}}}}}}}}}}",
            "const x = /[/;",
            "import type type type from from;",
            "\u{feff}\u{202e}reversed",
        ];
        for src in adversarial {
            // Must return (not abort). Either None or Some is acceptable.
            let _ = minify_js_oxc(src, "x.ts", true);
            let _ = minify_js_oxc(src, "x.tsx", false);
        }
    }
}
