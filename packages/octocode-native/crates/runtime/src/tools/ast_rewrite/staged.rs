//! One parse per staged file: the syntax-regression check and the
//! `remainingMatches` postcondition share the staged tree, and the source side
//! of the check reuses the scan's own parse.

use octocode_engine::structural::{
    CompiledRewrite, MAX_REWRITE_CONTENT_BYTES, compile_rewrite, count_syntax_errors,
    rewrite_parser_for_path,
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::HashMap};

use super::{RewriteError, RewriteRequest, compile_error, engine_error, rule_config};

thread_local! {
    /// Engine parses issued by astRewrite on this thread (tests assert the
    /// per-file parse budget).
    static PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn note_parses(count: usize) {
    PARSES.with(|parses| parses.set(parses.get().saturating_add(count)));
}

#[cfg(test)]
pub(super) fn take_parses() -> usize {
    PARSES.with(|parses| parses.replace(0))
}

/// Staged-content facts from its single parse.
#[derive(Clone, Copy, Debug)]
pub(super) struct StagedFacts {
    pub syntax_errors: u32,
    /// Rewrite matches left in the staged content; `None` when the rule could
    /// not be evaluated there (the postcondition then fails closed).
    pub remaining: Option<usize>,
}

/// Validated rule, compiled once per parser language for the request.
pub(super) struct StagedAnalyzer {
    selector: String,
    config: Value,
    compiled: RefCell<HashMap<String, Option<CompiledRewrite>>>,
}

impl StagedAnalyzer {
    /// Validates the rule by compiling it (no probe parse).
    pub(super) fn new(query: &RewriteRequest) -> Result<Self, RewriteError> {
        let config = rule_config(query);
        let compiled =
            compile_rewrite(config.clone()).map_err(|error| compile_error(query, error))?;
        Ok(Self {
            compiled: RefCell::new(HashMap::from([(query.lang().to_owned(), Some(compiled))])),
            selector: query.lang().to_owned(),
            config,
        })
    }

    pub(super) fn config(&self) -> &Value {
        &self.config
    }

    /// Same per-file parser the scan used (`.tsx` under `typescript` is TSX), so
    /// JSX is not counted as damage by a grammar that cannot parse it.
    fn parser(&self, path: &str) -> String {
        rewrite_parser_for_path(&self.selector, path)
    }

    /// Syntax errors in content the scan did not report on (fallback only).
    pub(super) fn count_errors(&self, path: &str, content: &str) -> Result<u32, RewriteError> {
        note_parses(1);
        count_syntax_errors(content, &self.parser(path)).map_err(engine_error)
    }

    /// Parse staged content once for its error count and remaining matches.
    /// `None` when it exceeds the engine's parse bound: an unverifiable but
    /// legal rewrite stages rather than hard-fails.
    pub(super) fn staged(
        &self,
        path: &str,
        content: &str,
    ) -> Result<Option<StagedFacts>, RewriteError> {
        if content.len() > MAX_REWRITE_CONTENT_BYTES {
            return Ok(None);
        }
        let parser = self.parser(path);
        let mut compiled = self.compiled.borrow_mut();
        let rewrite = compiled.entry(parser.clone()).or_insert_with(|| {
            let mut file_rule = self.config.clone();
            if let Some(object) = file_rule.as_object_mut() {
                object.insert("language".to_owned(), json!(parser));
            }
            compile_rewrite(file_rule).ok()
        });
        if let Some(scan) = rewrite.as_ref().and_then(|rewrite| {
            note_parses(1);
            rewrite.scan(content).ok()
        }) {
            return Ok(Some(StagedFacts {
                syntax_errors: scan.syntax_errors,
                remaining: Some(scan.matches.len()),
            }));
        }
        drop(compiled);
        Ok(Some(StagedFacts {
            syntax_errors: self.count_errors(path, content)?,
            remaining: None,
        }))
    }
}
