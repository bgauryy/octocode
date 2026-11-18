//! What a `testCode` scope left out of a listing walk, by walk position.
use octocode_engine::num::saturating_u32;
use octocode_runtime_base::policy::path::PathPolicy;

/// A `testCode` scope over a listing walk, and the walk positions of the
/// entries it left out: a window counts those it covers.
pub(super) struct TestCodeLeft {
    pub(super) scoped: std::sync::Arc<octocode_tools_shared::test_code::Scoped>,
    left: std::sync::Arc<std::sync::Mutex<Vec<u32>>>,
}

impl TestCodeLeft {
    pub(super) fn of(
        scope: Option<octocode_contract_types::TestCode>,
        paths: &PathPolicy,
    ) -> Option<Self> {
        octocode_tools_shared::test_code::Scoped::of(scope, paths).map(|scoped| Self {
            scoped: std::sync::Arc::new(scoped),
            left: std::sync::Arc::default(),
        })
    }

    /// The engine's listing rule: the scope's verdict, recording each entry
    /// left out by position (past `replayed`, which the window it resumes
    /// counted).
    pub(super) fn rule(&self, replayed: u32) -> octocode_engine::types::ListsEntry {
        let (scoped, left) = (
            std::sync::Arc::clone(&self.scoped),
            std::sync::Arc::clone(&self.left),
        );
        octocode_engine::types::ListsEntry(std::sync::Arc::new(move |path, is_dir, position| {
            let listed = scoped.lists(path, is_dir);
            if !listed && position > replayed {
                left.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(position);
            }
            listed
        }))
    }

    /// Count what earlier windows left out along with this walk's.
    pub(super) fn carry(&self, positions: &[u32]) {
        self.left
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend_from_slice(positions);
    }

    /// The walk positions left out so far.
    pub(super) fn positions(&self) -> Vec<u32> {
        self.left
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// What the window through walk position `through` states.
    pub(super) fn window(self, through: usize) -> super::super::super::walk_policy::TestCodeWindow {
        super::super::super::walk_policy::TestCodeWindow {
            unread: self.scoped.unread(),
            excluded: self.through(through),
        }
    }

    /// The entries left out at walk positions through `through`.
    fn through(self, through: usize) -> octocode_engine::types::TestCodeExcluded {
        let left = self
            .left
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        octocode_engine::types::TestCodeExcluded {
            files: saturating_u32(
                left.iter()
                    .filter(|position| {
                        octocode_engine::num::saturating_usize(**position) <= through
                    })
                    .count(),
            ),
            lines: 0,
        }
    }
}
