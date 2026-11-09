//! The identity of one complete acquisition (`resource::acquisition_key`).
use super::*;
use crate::policy::path::PathPolicyConfig;
use crate::response::result::canonical_tests::{Sampled, variants};
use crate::tools::judgment::Evaluator;

/// A reader that only answers what keying asks of one: its path policy and
/// its input security (and a glob's expansion, its path policy). Any other
/// service is a test bug.
pub(super) struct Keying {
    paths: PathPolicy,
    security: ContentSecurity,
    dir: tempfile::TempDir,
}

impl Keying {
    pub(super) fn new() -> Self {
        let root = tempfile::tempdir().expect("workspace");
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().canonicalize().expect("canonical")),
            ..Default::default()
        })
        .expect("policy");
        Self {
            paths,
            security: ContentSecurity::new(),
            dir: root,
        }
    }

    /// The workspace root the policy admits.
    pub(super) fn root(&self) -> &Path {
        self.dir.path()
    }
}

impl SourceReader for Keying {
    fn paths(&self) -> &PathPolicy {
        &self.paths
    }
    fn read_paths(&self) -> &PathPolicy {
        &self.paths
    }
    fn security(&self) -> &ContentSecurity {
        &self.security
    }
    fn config(&self) -> &ConfigOutput {
        unreachable!("keying reads no config")
    }
    fn limits(&self) -> &RuntimeLimits {
        unreachable!("keying reads no limits")
    }
    fn home(&self) -> &Path {
        unreachable!("keying reads no home")
    }
    fn handle(&self) -> &tokio::runtime::Handle {
        unreachable!("keying runs nothing")
    }
    fn availability(&self) -> &ToolAvailability {
        unreachable!("keying checks no availability")
    }
    fn evaluator(&self) -> &Evaluator {
        unreachable!("keying judges nothing")
    }
    fn output(&self, tool: ToolId) -> &'static dyn crate::response::output::ToolOutput {
        tool.output()
    }
    fn execute(
        &self,
        _: ToolId,
        _: &CheckedQuery,
        _: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        unreachable!("keying runs no row")
    }
    fn retained_github_identity(
        &self,
        _: &ExecutionContext,
    ) -> Result<Option<String>, ClassificationError> {
        unreachable!("keying reads no identity")
    }
    fn validate_retained_github_identity(
        &self,
        _: Option<&str>,
        _: &ExecutionContext,
    ) -> Result<(), ClassificationError> {
        unreachable!("keying checks no identity")
    }
    fn text_capture_limit(&self) -> usize {
        unreachable!("keying captures nothing")
    }
    fn capture_local_text(
        &self,
        _: &Value,
        _: &ExecutionContext,
    ) -> Result<(String, PathBuf, String), ClassificationError> {
        unreachable!("keying captures nothing")
    }
    fn capture_github_content(
        &self,
        _: &Value,
        _: &ExecutionContext,
        _: usize,
    ) -> Result<octocode_github::ContentResponse, ClassificationError> {
        unreachable!("keying captures nothing")
    }
}

/// A reader row stating a field at its contract default is the same
/// acquisition as the row omitting it, for every form of each resource
/// reader the contract declares; any other row is another acquisition.
#[test]
fn a_field_stated_at_its_default_is_the_same_acquisition_as_its_omission() {
    let reader = Keying::new();
    for tool in [ToolId::LocalFetch, ToolId::GhGetFileContent] {
        let name = tool.as_str();
        let found = variants(tool);
        assert!(found.skipped.is_empty(), "{name}: {:?}", found.skipped);
        let key = |row: &Value| {
            super::super::resource::acquisition_key(&json!({"tool": name, "query": row}), &reader)
                .unwrap_or_else(|error| panic!("{name}: {row} has no key: {error:?}"))
        };
        let mut stated_a_default = 0;
        for Sampled { stated, omitted } in found.rows {
            assert_eq!(key(&stated), key(&omitted), "{name}: {stated} vs {omitted}");
            let mut other = omitted.clone();
            other["path"] = json!("another/file.rs");
            assert_ne!(key(&omitted), key(&other), "{name}: the path is identity");
            stated_a_default += usize::from(stated != omitted);
        }
        assert!(stated_a_default > 0, "{name}: no form states a default");
    }
}
