use crate::types::{
    FileSystemQueryOptions, FileSystemQueryResult, GraphFactsScanOptions, GraphFactsScanResult,
    IndexBuildRequest, IndexBuildResult, IndexQueryRequest, IndexQueryResult, IndexStatusRequest,
    IndexStatusResult, MinifyResult, RipgrepParseResult, RipgrepSearchOptions,
};
use napi::{Env, Error, Result, Status, Task};

pub struct BuildIndexTask {
    pub options: Option<IndexBuildRequest>,
}

impl Task for BuildIndexTask {
    type Output = IndexBuildResult;
    type JsValue = IndexBuildResult;

    fn compute(&mut self) -> Result<Self::Output> {
        let options = self.options.take().ok_or_else(|| {
            Error::new(
                Status::GenericFailure,
                "index build options already consumed",
            )
        })?;
        crate::bindings::index::build_index_inner(options)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct QueryIndexTask {
    pub options: Option<IndexQueryRequest>,
}

impl Task for QueryIndexTask {
    type Output = IndexQueryResult;
    type JsValue = IndexQueryResult;

    fn compute(&mut self) -> Result<Self::Output> {
        let options = self.options.take().ok_or_else(|| {
            Error::new(
                Status::GenericFailure,
                "index query options already consumed",
            )
        })?;
        crate::bindings::index::query_index_inner(options)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct IndexStatusTask {
    pub options: Option<IndexStatusRequest>,
}

impl Task for IndexStatusTask {
    type Output = IndexStatusResult;
    type JsValue = IndexStatusResult;

    fn compute(&mut self) -> Result<Self::Output> {
        let options = self.options.take().ok_or_else(|| {
            Error::new(
                Status::GenericFailure,
                "index status options already consumed",
            )
        })?;
        crate::bindings::index::index_status_inner(options)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct MinifyContentTask {
    pub content: String,
    pub file_path: String,
}

impl Task for MinifyContentTask {
    type Output = MinifyResult;
    type JsValue = MinifyResult;

    fn compute(&mut self) -> Result<Self::Output> {
        Ok(crate::portable::minify_content(
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

pub struct FileSystemQueryTask {
    pub options: Option<FileSystemQueryOptions>,
}

impl Task for FileSystemQueryTask {
    type Output = FileSystemQueryResult;
    type JsValue = FileSystemQueryResult;

    fn compute(&mut self) -> Result<Self::Output> {
        let options = self.options.take().ok_or_else(|| {
            Error::new(
                Status::GenericFailure,
                "filesystem query options already consumed",
            )
        })?;
        Ok(crate::portable::query_file_system(options)?)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct GraphFactsScanTask {
    pub options: Option<GraphFactsScanOptions>,
}

impl Task for GraphFactsScanTask {
    type Output = GraphFactsScanResult;
    type JsValue = GraphFactsScanResult;

    fn compute(&mut self) -> Result<Self::Output> {
        let options = self.options.take().ok_or_else(|| {
            Error::new(
                Status::GenericFailure,
                "graph scan options already consumed",
            )
        })?;
        Ok(crate::portable::scan_graph_facts(options)?)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
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
        Ok(crate::portable::search_ripgrep(options)?)
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
        Ok(crate::portable::structural_search(
            &self.content,
            &self.file_path,
            self.pattern.as_deref(),
            self.rule.as_deref(),
        )?)
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
        Ok(crate::portable::structural_search_files(options)?)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct SemanticBoundaryOffsetsTask {
    pub content: String,
    pub file_path: String,
}

pub struct SyntaxTreeInspectTask {
    pub content: String,
    pub file_path: String,
    pub options: Option<crate::structural::SyntaxTreeInspectOptions>,
}

impl Task for SyntaxTreeInspectTask {
    type Output = crate::structural::SyntaxTreeInspectResult;
    type JsValue = crate::structural::SyntaxTreeInspectResult;

    fn compute(&mut self) -> Result<Self::Output> {
        let options = self.options.take();
        Ok(crate::portable::inspect_syntax_tree(
            &self.content,
            &self.file_path,
            options,
        )?)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

impl Task for SemanticBoundaryOffsetsTask {
    type Output = Vec<u32>;
    type JsValue = Vec<u32>;

    fn compute(&mut self) -> Result<Self::Output> {
        Ok(crate::portable::semantic_boundary_offsets(
            &self.content,
            &self.file_path,
        )?)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

#[cfg(feature = "embedded-ast-grep-rewrite")]
pub struct StructuralRewriteFilesTask {
    pub options: Option<crate::structural::StructuralRewriteFilesOptions>,
}

#[cfg(feature = "embedded-ast-grep-rewrite")]
impl Task for StructuralRewriteFilesTask {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> Result<Self::Output> {
        let options = self.options.take().ok_or_else(|| {
            Error::new(
                Status::GenericFailure,
                "structural rewrite files options already consumed",
            )
        })?;
        Ok(crate::portable::structural_rewrite_files(options)?)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}
