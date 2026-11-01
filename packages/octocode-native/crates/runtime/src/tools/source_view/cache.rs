use crate::cache::Store;
use crate::tools::declarations_cache::{SymbolsView, symbols_store};
use std::path::PathBuf;

/// Rendered source views owned by a runtime. Standalone readers default to
/// memory storage; runtime construction supplies the resolved disk policy.
pub struct ViewCache {
    pub(crate) symbols: Store<SymbolsView>,
}

impl ViewCache {
    pub(crate) fn new(directory: Option<PathBuf>) -> Self {
        Self {
            symbols: symbols_store(directory),
        }
    }

    pub(crate) fn clear_memory(&self) {
        self.symbols.clear_memory();
    }
}

impl Default for ViewCache {
    fn default() -> Self {
        Self::new(None)
    }
}

impl std::fmt::Debug for ViewCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ViewCache").finish_non_exhaustive()
    }
}
