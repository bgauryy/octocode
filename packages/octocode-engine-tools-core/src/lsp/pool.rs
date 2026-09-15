//! Process-local language-server clients, one per workspace command.
use octocode_engine::lsp::client::NativeLspClient;
use octocode_engine::lsp::types::JsLanguageServerConfig;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

const MAX_CLIENTS: usize = 8;

#[derive(Default)]
pub struct LspPool {
    inner: Mutex<PoolInner>,
}

#[derive(Default)]
struct PoolInner {
    clients: HashMap<String, Arc<NativeLspClient>>,
    order: VecDeque<String>,
}

impl LspPool {
    pub fn get_or_insert(&self, config: JsLanguageServerConfig) -> Arc<NativeLspClient> {
        let fingerprint = config
            .initialization_options
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        let key = format!(
            "{}::{}::{fingerprint}",
            config.workspace_root, config.command
        );
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(existing) = inner.clients.get(&key) {
            let client = Arc::clone(existing);
            touch(&mut inner.order, &key);
            return client;
        }
        while inner.clients.len() >= MAX_CLIENTS {
            if let Some(oldest) = inner.order.pop_front() {
                inner.clients.remove(&oldest);
            } else {
                break;
            }
        }
        let client = Arc::new(NativeLspClient::new(config));
        inner.clients.insert(key.clone(), Arc::clone(&client));
        inner.order.push_back(key);
        client
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clients
            .len()
    }
}

fn touch(order: &mut VecDeque<String>, key: &str) {
    order.retain(|value| value != key);
    order.push_back(key.to_owned());
}

#[cfg(test)]
mod tests {
    use super::*;
    use octocode_engine::lsp::types::JsLanguageServerConfig;

    fn config(root: &str, command: &str, fingerprint: Option<&str>) -> JsLanguageServerConfig {
        JsLanguageServerConfig {
            command: command.into(),
            args: None,
            workspace_root: root.into(),
            language_id: Some("rust".into()),
            initialization_options: fingerprint.map(|value| serde_json::json!({ "id": value })),
            env: None,
        }
    }

    #[test]
    fn reuses_the_same_client_for_workspace_command_and_init_options() {
        let pool = LspPool::default();
        let first = pool.get_or_insert(config("/repo", "rust-analyzer", Some("a")));
        let second = pool.get_or_insert(config("/repo", "rust-analyzer", Some("a")));
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn partitions_clients_by_workspace_command_and_init_options() {
        let pool = LspPool::default();
        let rust = pool.get_or_insert(config("/repo", "rust-analyzer", None));
        let other_root = pool.get_or_insert(config("/other", "rust-analyzer", None));
        let ts = pool.get_or_insert(config("/repo", "typescript-language-server", None));
        let rust_ctx = pool.get_or_insert(config("/repo", "rust-analyzer", Some("features")));
        assert!(!Arc::ptr_eq(&rust, &other_root));
        assert!(!Arc::ptr_eq(&rust, &ts));
        assert!(!Arc::ptr_eq(&rust, &rust_ctx));
    }

    #[test]
    fn evicts_the_least_recently_used_client_at_capacity() {
        let pool = LspPool::default();
        let first = pool.get_or_insert(config("/repo", "first", None));
        for index in 0..8 {
            let _ = pool.get_or_insert(config(&format!("/repo{index}"), "cmd", None));
        }
        assert_eq!(pool.len(), 8);
        let again = pool.get_or_insert(config("/repo", "first", None));
        assert!(!Arc::ptr_eq(&first, &again));
    }
}
