//! Issued handles for saved tool work and immutable response pages.
//! A handle selects a member of one retained set. It never contains caller-editable progress.
use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};
use crate::id::ToolId;
use crate::response::cursor::Cursor;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const PREFIX: &str = "cursor1.";
const TTL: Duration = Duration::from_secs(120);

#[derive(Clone, Serialize, Deserialize)]
pub(super) enum SavedWork {
    ToolResume,
    ResponsePage(Cursor),
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Member {
    pub tool: String,
    /// Complete normalized input, including contract-typed private progress.
    pub input: Value,
    pub work: SavedWork,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct CursorSet {
    access: [u8; 32],
    pub github_identity: Option<String>,
    members: BTreeMap<String, Member>,
}

pub(super) struct Registry {
    store: Store<CursorSet>,
    policy: String,
}

pub(super) struct Pending {
    id: String,
    set: CursorSet,
}

pub(super) struct Opened {
    set: Arc<CursorSet>,
    member: String,
    id: String,
}

impl Opened {
    pub fn pending(&self) -> Pending {
        Pending {
            id: self.id.clone(),
            set: (*self.set).clone(),
        }
    }
    pub fn member(&self) -> &Member {
        &self.set.members[&self.member]
    }
    pub fn github_identity(&self) -> Option<&str> {
        self.set.github_identity.as_deref()
    }
}

impl Registry {
    pub fn new(config: CacheConfig, disk: Option<PathBuf>, policy: String) -> Self {
        Self {
            store: Store::new(config, disk),
            policy,
        }
    }

    fn key(&self, id: &str) -> CacheKey {
        CacheKey {
            namespace: "issued-continuations-v1".into(),
            scope: self.policy.clone(),
            resource: id.into(),
        }
    }

    pub fn clear_memory(&self) {
        self.store.clear_memory();
    }

    /// Every handle in one output becomes reachable atomically, before publication.
    pub fn retain(&self, pending: Pending) -> bool {
        if pending.set.members.is_empty() {
            return true;
        }
        let bytes = octocode_engine::json_size::bytes(&pending.set).unwrap_or(usize::MAX);
        self.store
            .put_confirmed(
                self.key(&pending.id),
                pending.set,
                bytes,
                CacheClass::Volatile { ttl: TTL },
            )
            .1
    }

    pub fn open(&self, tool: ToolId, token: &str, access: [u8; 32]) -> Option<Opened> {
        let encoded = token.strip_prefix(PREFIX)?;
        if encoded.len() != 64 || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let (id, member) = encoded.split_at(32);
        let set = self.store.get(&self.key(id))?.value;
        if set.access != access || set.members.get(member)?.tool != tool.as_str() {
            return None;
        }
        Some(Opened {
            set,
            member: member.into(),
            id: id.into(),
        })
    }
}

impl Pending {
    pub fn is_empty(&self) -> bool {
        self.set.members.is_empty()
    }

    pub fn bind_github_identity(&mut self, identity: Option<String>) {
        self.set.github_identity = identity;
    }

    pub fn new(access: [u8; 32], github_identity: Option<String>) -> Option<Self> {
        Some(Self {
            id: random_id()?,
            set: CursorSet {
                access,
                github_identity,
                members: BTreeMap::new(),
            },
        })
    }

    pub fn insert(&mut self, tool: ToolId, input: Value, work: SavedWork) -> Option<String> {
        let id = random_id()?;
        let token = format!("{PREFIX}{}{id}", self.id);
        self.set.members.insert(
            id,
            Member {
                tool: tool.as_str().into(),
                input,
                work,
            },
        );
        Some(token)
    }
}

fn random_id() -> Option<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    Some(hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn registry(policy: &str) -> Registry {
        Registry::new(
            CacheConfig {
                max_entries: 8,
                max_bytes: 1024 * 1024,
                disk_bytes: 1024 * 1024,
            },
            None,
            policy.into(),
        )
    }

    #[test]
    fn issued_work_retains_the_complete_request_and_rejects_other_access_or_tool() {
        let registry = registry("contract-and-policy");
        let input = json!({"queries":[{"path":"src","matchString":["exact"],"page":2}],"responseChars":1024});
        let mut pending = Pending::new([7; 32], Some("github-user".into())).expect("randomness");
        let token = pending
            .insert(ToolId::LocalSearch, input.clone(), SavedWork::ToolResume)
            .expect("randomness");
        assert!(
            registry
                .open(ToolId::LocalSearch, &token, [7; 32])
                .is_none()
        );
        assert!(registry.retain(pending));
        let opened = registry
            .open(ToolId::LocalSearch, &token, [7; 32])
            .expect("issued handle");
        assert_eq!(opened.member().input, input);
        assert_eq!(opened.github_identity(), Some("github-user"));
        assert!(registry.open(ToolId::LocalFetch, &token, [7; 32]).is_none());
        assert!(
            registry
                .open(ToolId::LocalSearch, &token, [8; 32])
                .is_none()
        );
        assert!(
            registry
                .open(ToolId::LocalSearch, &format!("{token}0"), [7; 32])
                .is_none()
        );
        assert!(
            registry
                .open(ToolId::LocalSearch, "c1.eyJzdGF0ZSI6e319", [7; 32])
                .is_none()
        );
    }

    #[test]
    fn a_set_retains_many_members_without_evicting_its_own_handles() {
        let registry = registry("policy");
        let mut pending = Pending::new([0; 32], None).expect("randomness");
        let tokens = (0..30)
            .map(|page| {
                pending
                    .insert(
                        ToolId::LocalSearch,
                        json!({"queries":[{"path":"src","page":page}]}),
                        SavedWork::ToolResume,
                    )
                    .expect("randomness")
            })
            .collect::<Vec<_>>();
        assert!(registry.retain(pending));
        for token in tokens {
            assert!(
                registry
                    .open(ToolId::LocalSearch, &token, [0; 32])
                    .is_some()
            );
        }
    }
    #[test]
    fn disk_reopen_preserves_work_but_different_contract_or_policy_cannot_open_it() {
        let disk = tempfile::tempdir().expect("disk");
        let config = CacheConfig {
            max_entries: 8,
            max_bytes: 1024 * 1024,
            disk_bytes: 1024 * 1024,
        };
        let first = Registry::new(
            config.clone(),
            Some(disk.path().into()),
            "contract-a;policy-a".into(),
        );
        let input = json!({"queries":[{"path":"a.ts","ranges":[{"startLine":2,"endLine":7}],"readProgress":{"unit":"lines","offset":4},"snapshot":"source-pin"}],"responseChars":3000});
        let mut pending = Pending::new([3; 32], Some("account-a".into())).expect("randomness");
        let token = pending
            .insert(ToolId::LocalFetch, input.clone(), SavedWork::ToolResume)
            .expect("randomness");
        assert!(first.retain(pending));
        first.clear_memory();
        let second = Registry::new(
            config.clone(),
            Some(disk.path().into()),
            "contract-a;policy-a".into(),
        );
        let opened = second
            .open(ToolId::LocalFetch, &token, [3; 32])
            .expect("disk replay");
        assert_eq!(opened.member().input, input);
        assert_eq!(opened.github_identity(), Some("account-a"));
        let changed = Registry::new(
            config,
            Some(disk.path().into()),
            "contract-b;policy-a".into(),
        );
        assert!(changed.open(ToolId::LocalFetch, &token, [3; 32]).is_none());
        let mut altered = token.clone().into_bytes();
        let at = altered.len() - 1;
        altered[at] = if altered[at] == b'0' { b'1' } else { b'0' };
        assert!(
            second
                .open(
                    ToolId::LocalFetch,
                    std::str::from_utf8(&altered).expect("ascii"),
                    [3; 32]
                )
                .is_none()
        );
    }

    #[test]
    fn unavailable_storage_never_claims_an_issued_handle() {
        let registry = Registry::new(
            CacheConfig {
                max_entries: 0,
                max_bytes: 32,
                disk_bytes: 0,
            },
            None,
            "policy".into(),
        );
        let mut pending = Pending::new([0; 32], None).expect("randomness");
        let token = pending
            .insert(
                ToolId::LocalSearch,
                json!({"queries":[{"path":"src","matchString":["run"]}]}),
                SavedWork::ToolResume,
            )
            .expect("randomness");
        assert!(!registry.retain(pending));
        assert!(
            registry
                .open(ToolId::LocalSearch, &token, [0; 32])
                .is_none()
        );
    }

    #[test]
    fn extending_saved_response_pages_retains_earlier_source_work() {
        let registry = registry("policy");
        let mut pending = Pending::new([1; 32], None).expect("randomness");
        let first = pending
            .insert(
                ToolId::LocalSearch,
                json!({"queries":[{"path":"src","matchString":["run"],"page":2}]}),
                SavedWork::ToolResume,
            )
            .expect("randomness");
        assert!(registry.retain(pending));
        let opened = registry
            .open(ToolId::LocalSearch, &first, [1; 32])
            .expect("first");
        let mut pending = opened.pending();
        let next = pending
            .insert(
                ToolId::LocalFetch,
                json!({"queries":[{"path":"src/a.ts"}]}),
                SavedWork::ToolResume,
            )
            .expect("randomness");
        assert!(registry.retain(pending));
        assert!(
            registry
                .open(ToolId::LocalSearch, &first, [1; 32])
                .is_some()
        );
        assert!(registry.open(ToolId::LocalFetch, &next, [1; 32]).is_some());
    }
}
