use crate::comment_remover::remove_comments;

// ── OXC JS/TS AST minifier ────────────────────────────────────────────────────

/// OXC-backed JS/TS minification.
///
/// `mangle = true`: rename locals (maximum compression, for full minify path).
/// `mangle = false`: preserve names (agent-readable, for content-view path).
///
/// Returns `None` on parse error so callers can choose the non-OXC path.
pub fn minify_js_oxc(content: &str, file_path: &str, mangle: bool) -> Option<String> {
    use oxc_allocator::Allocator;
    use oxc_codegen::{Codegen, CodegenOptions, CommentOptions};
    use oxc_minifier::{CompressOptions, MangleOptions, Minifier, MinifierOptions};
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    let allocator = Allocator::default();
    let ext = crate::file_extension::get_extension_internal(file_path, true, "js");
    let source_type = match ext.as_str() {
        "ts" => SourceType::ts(),
        "tsx" => SourceType::tsx(),
        "jsx" => SourceType::jsx(),
        "mjs" => SourceType::mjs(),
        _ => SourceType::default(), // js / cjs
    };

    let parser_ret = Parser::new(&allocator, content, source_type).parse();
    // If plain-JS parse failed (e.g. Flow-annotated file with `import type`), retry as TS.
    let parser_ret = if !parser_ret.errors.is_empty() && matches!(ext.as_str(), "js" | "cjs") {
        Parser::new(&allocator, content, SourceType::ts()).parse()
    } else {
        parser_ret
    };
    if !parser_ret.errors.is_empty() {
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
    let s = super::collapse_whitespace(&s);
    let s = re_tighten_punct_js(&s);
    // Split back to lines, drop empty
    s.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn re_tighten_punct_js(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
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
