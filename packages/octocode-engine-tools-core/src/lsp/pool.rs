//! Process-local language-server clients, one per workspace command.
use octocode_engine::lsp::client::NativeLspClient;
use octocode_engine::lsp::types::JsLanguageServerConfig;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct LspPool {
    clients: Mutex<HashMap<String, Arc<NativeLspClient>>>,
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
        let mut clients = self
            .clients
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(existing) = clients.get(&key) {
            return Arc::clone(existing);
        }
        let client = Arc::new(NativeLspClient::new(config));
        clients.insert(key, Arc::clone(&client));
        client
    }
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
}
