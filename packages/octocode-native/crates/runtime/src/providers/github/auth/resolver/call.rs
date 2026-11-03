//! The credential selection one tool call shares across its rows and its
//! saved-response identity checks. It is request-owned: it lives and dies
//! with the call, so a login, logout or rotation is observed by the next call.
use super::{AuthBackend, AuthMode, AuthSelection, Authentication, SystemBackend};
use octocode_github::{ProviderError, ResolvedCredential};
use octocode_io::budget::RequestBudget;
use std::sync::{Arc, Mutex};
use tokio::sync::OnceCell;

#[derive(Default)]
struct Selected {
    /// The selection every row of the call sends; refresh runs at most once.
    request: Arc<OnceCell<Option<AuthSelection>>>,
    /// The read-only identity of the sources, for a call that sent no row.
    inspected: Arc<OnceCell<String>>,
}

/// One call's credential selection. Concurrent rows wait for the first
/// resolution instead of repeating the keychain read or `gh auth token` spawn;
/// a failed or cancelled resolution is not kept, so the next row retries.
#[derive(Default)]
pub(crate) struct CallSelection(Mutex<Selected>);

impl CallSelection {
    fn current(&self) -> (Arc<OnceCell<Option<AuthSelection>>>, Arc<OnceCell<String>>) {
        let selected = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (selected.request.clone(), selected.inspected.clone())
    }

    /// GitHub rejected what the call selected: the next row selects afresh.
    fn forget(&self) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Selected::default();
    }
}

impl Authentication {
    /// The credential `call` sends: resolved by its first row, shared by the rest.
    pub(crate) async fn resolve_shared(
        &self,
        call: &CallSelection,
        host: &str,
        budget: &RequestBudget,
    ) -> Result<Option<AuthSelection>, ProviderError> {
        self.resolve_shared_with(
            call,
            host,
            budget,
            &SystemBackend {
                config: &self.config,
            },
        )
        .await
    }

    pub(super) async fn resolve_shared_with(
        &self,
        call: &CallSelection,
        host: &str,
        budget: &RequestBudget,
        backend: &impl AuthBackend,
    ) -> Result<Option<AuthSelection>, ProviderError> {
        let (request, _) = call.current();
        request
            .get_or_try_init(|| self.resolve_with(host, AuthMode::Request, budget, backend))
            .await
            .cloned()
    }

    /// The identity of the credential `call` used, else of the current
    /// sources, inspected once. A rejected credential has no identity.
    pub(crate) async fn identity_shared(
        &self,
        call: &CallSelection,
        host: &str,
        budget: &RequestBudget,
    ) -> Result<String, ProviderError> {
        self.identity_shared_with(
            call,
            host,
            budget,
            &SystemBackend {
                config: &self.config,
            },
        )
        .await
    }

    pub(super) async fn identity_shared_with(
        &self,
        call: &CallSelection,
        host: &str,
        budget: &RequestBudget,
        backend: &impl AuthBackend,
    ) -> Result<String, ProviderError> {
        let (request, inspected) = call.current();
        if let Some(selection) = request.get() {
            return self.selection_identity(host, selection.as_ref());
        }
        inspected
            .get_or_try_init(|| self.inspect_identity_with(host, budget, backend))
            .await
            .cloned()
    }

    /// GitHub rejected `credential` for a row of `call`: later calls skip it
    /// ([`Self::reject`]) and later rows of this call select again.
    pub(crate) fn reject_shared(
        &self,
        call: &CallSelection,
        host: &str,
        credential: Option<&ResolvedCredential>,
    ) {
        self.reject(host, credential);
        call.forget();
    }
}
