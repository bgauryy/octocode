//! The dialect oxc reads a file as, the worker its jobs run on, and the parse
//! they read.

use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::deep_stack::{job_cancelled, run_on_deep_stack, with_thread_allocator};
use crate::text::JS_TS_EXTENSIONS;

pub(super) fn source_type_for(ext: &str, file_path: &str, content: &str) -> SourceType {
    // oxc has no Flow parser. Flow annotations are close enough to TypeScript
    // that a TSX parse usually succeeds; when it cannot, the hard failure
    // routes the caller to the tree-sitter TSX grammar (see `grammar_extension`).
    if crate::text::file_extension::is_flow_source(content, ext) {
        return SourceType::tsx();
    }
    // `from_path` owns the extension policy: `.cjs`/`.cts` are CommonJS (top-level
    // `return` allowed), `.mjs`/`.mts` are ES modules, `.js`/`.ts` are
    // unambiguous (module only with import/export), and `.d.ts`/`.d.mts`/`.d.cts`
    // are ambient declarations. Paths without a JS/TS extension fall back to the
    // caller's defaulted extension (`extension_of(.., "ts")`).
    // A parser override (`ext` other than the path's own) decides the dialect.
    let own_dialect = crate::text::extension_of(file_path, true, "ts") == ext;
    let source_type = own_dialect
        .then(|| SourceType::from_path(file_path).ok())
        .flatten()
        .or_else(|| SourceType::from_extension(ext).ok())
        .unwrap_or_else(SourceType::ts);
    // React components commonly live in plain `.js`/`.mjs`/`.cjs`; JSX there must
    // not be a parse error. TypeScript stays JSX-free so `<T>x` assertions parse.
    if source_type.is_javascript() {
        source_type.with_jsx(true)
    } else {
        source_type
    }
}

/// The extension naming the dialect of a file: its parser override, else its
/// own. An override selects a grammar and leaves the extraction to that
/// grammar's owner, so `typescript` on a `.ts` file reads as a plain `.ts`.
fn dialect_extension(file_path: &str, parser: Option<&str>) -> String {
    parser.map_or_else(
        || crate::text::extension_of(file_path, true, "ts"),
        str::to_ascii_lowercase,
    )
}

/// Cheap pre-check so non-JS/TS files never pay for the content copy and the
/// big-stack worker thread; graph scans call this for every file in a repo.
pub(super) fn is_oxc_dialect(file_path: &str, parser: Option<&str>) -> bool {
    JS_TS_EXTENSIONS.contains(&dialect_extension(file_path, parser).as_str())
}

/// Run an oxc `job` on the deep-stack pool to completion: one file cannot be
/// paged, and input is bounded by `MAX_PARSE_SIZE` (see
/// `extractor::whole_file_deadline`). `None` for oversized input or a
/// non-JS/TS path. oxc can ICE on pathological input; the unwind is contained
/// so it never aborts the process.
pub(super) fn on_oxc_worker<T: Send + 'static>(
    content: &str,
    file_path: &str,
    parser: Option<&str>,
    job: impl FnOnce(&str, &str) -> Option<T> + Send + 'static,
) -> Option<T> {
    if content.len() > crate::signatures::MAX_PARSE_SIZE || !is_oxc_dialect(file_path, parser) {
        return None;
    }
    let content = content.to_owned();
    let file_path = file_path.to_owned();
    run_on_deep_stack(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&content, &file_path)))
            .unwrap_or(None)
    })
}

/// Parse JS/TS `content` on the thread arena (`TOKENS` keeps the token
/// stream) and hand `f` the extension and the parse. `None` for a non-JS/TS
/// extension, a cancelled job, or a hard parse failure with nothing
/// recovered: the caller then falls back to the more error-tolerant
/// tree-sitter path rather than emit a stub.
pub(super) fn with_js_program<const TOKENS: bool, R>(
    content: &str,
    file_path: &str,
    parser: Option<&str>,
    f: impl for<'a> FnOnce(String, oxc_parser::ParserReturn<'a>) -> Option<R>,
) -> Option<R> {
    let ext = dialect_extension(file_path, parser);
    if !JS_TS_EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }
    with_thread_allocator(|allocator| {
        let parser = Parser::new(
            allocator,
            content,
            source_type_for(&ext, file_path, content),
        );
        let parser_ret = if TOKENS {
            parser
                .with_config(oxc_parser::config::TokensParserConfig)
                .parse()
        } else {
            parser.parse()
        };
        if job_cancelled()
            || (parser_ret.program.body.is_empty() && !parser_ret.diagnostics.is_empty())
        {
            return None;
        }
        f(ext, parser_ret)
    })
}
