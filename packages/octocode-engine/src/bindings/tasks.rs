use crate::ripgrep_search;
use crate::types::{MinifyResult, RipgrepParseResult, RipgrepSearchOptions};
use napi::{Env, Error, Result, Status, Task};

pub struct MinifyContentTask {
    pub content: String,
    pub file_path: String,
}

impl Task for MinifyContentTask {
    type Output = MinifyResult;
    type JsValue = MinifyResult;

    fn compute(&mut self) -> Result<Self::Output> {
        Ok(crate::minifier::minify_content_result_inner(
            &self.content,
            &self.file_path,
        ))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct SearchRipgrepTask {
    pub options: Option<RipgrepSearchOptions>,
}

impl Task for SearchRipgrepTask {
    type Output = RipgrepParseResult;
    type JsValue = RipgrepParseResult;

    fn compute(&mut self) -> Result<Self::Output> {
        // `compute` runs on the libuv thread pool, so the filesystem walk never
        // blocks the Node event loop. `options` is moved out on first call.
        let options = self
            .options
            .take()
            .ok_or_else(|| Error::new(Status::GenericFailure, "search options already consumed"))?;
        ripgrep_search::search(options)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct StructuralSearchTask {
    pub content: String,
    pub file_path: String,
    pub pattern: Option<String>,
    pub rule: Option<String>,
}

impl Task for StructuralSearchTask {
    type Output = Vec<crate::structural::StructuralMatch>;
    type JsValue = Vec<crate::structural::StructuralMatch>;

    fn compute(&mut self) -> Result<Self::Output> {
        let ext = crate::file_extension::get_extension_internal(&self.file_path, true, "txt");
        // Same panic guard as the (formerly) sync binding: an unwind across the
        // napi FFI boundary would abort the Node process.
        std::panic::catch_unwind(|| {
            crate::structural::search(
                &self.content,
                &ext,
                self.pattern.as_deref(),
                self.rule.as_deref(),
            )
        })
        .unwrap_or_else(|_| Err("structural search failed on pathological input".to_string()))
        .map_err(|message| Error::new(Status::InvalidArg, message))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct StructuralSearchFilesTask {
    pub options: Option<crate::structural::StructuralSearchFilesOptions>,
}

impl Task for StructuralSearchFilesTask {
    type Output = crate::structural::StructuralSearchFilesResult;
    type JsValue = crate::structural::StructuralSearchFilesResult;

    fn compute(&mut self) -> Result<Self::Output> {
        let options = self.options.take().ok_or_else(|| {
            Error::new(
                Status::GenericFailure,
                "structural search options already consumed",
            )
        })?;
        std::panic::catch_unwind(|| crate::structural::search_files(options))
            .unwrap_or_else(|_| {
                Err("structural file search failed on pathological input".to_string())
            })
            .map_err(|message| Error::new(Status::InvalidArg, message))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct SemanticBoundaryOffsetsTask {
    pub content: String,
    pub file_path: String,
}

impl Task for SemanticBoundaryOffsetsTask {
    type Output = Vec<u32>;
    type JsValue = Vec<u32>;

    fn compute(&mut self) -> Result<Self::Output> {
        // Tree-sitter parsing is CPU-bound and, like the structural/signature
        // paths, can unwind on pathological input — a panic across the napi FFI
        // boundary would abort Node, so contain it here.
        std::panic::catch_unwind(|| {
            crate::signatures::get_semantic_boundary_offsets_inner(&self.content, &self.file_path)
        })
        .map_err(|_| {
            Error::new(
                Status::GenericFailure,
                "semantic boundary detection failed on pathological input",
            )
        })
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}
