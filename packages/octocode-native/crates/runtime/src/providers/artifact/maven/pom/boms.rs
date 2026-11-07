//! Imported BOMs (`<scope>import</scope>` in `<dependencyManagement>`),
//! applied as Maven applies them: depth-first, a BOM's own imports counting
//! as defined where it is imported, before the next import
//! (<https://maven.apache.org/guides/introduction/introduction-to-dependency-mechanism.html#importing-dependencies>).
//! Each POM is read once, and the BOMs one step down the walk are fetched
//! together.
use super::super::super::ArtifactError;
use super::super::super::http::RegistryClient;
use super::dependencies::{chain_properties, imports};
use super::{Fetched, Parent, fetch_pom, located};
use octocode_contract_types::ErrorCode;
use std::collections::{HashMap, HashSet};

/// What happened to a POM that was not read, as the `unread` line says it.
const MISSING: &str = "is not in the Maven repository";

/// One imported BOM.
struct Node {
    bom: Parent,
    /// The keys of the BOM's POM and its parents read so far, nearest first.
    chain: Vec<String>,
    /// The POM to read next: the BOM's own, then each parent; `None` once
    /// the chain is whole.
    next: Option<Parent>,
    /// The BOMs it imports, in declared order, once its chain is whole.
    imports: Vec<String>,
    /// Its own POM could not be read (stated in `unread`).
    failed: bool,
}

/// The BOMs a POM imports, read as the lookup needs them.
pub(super) struct Walk {
    /// The project chain's imports, in declared order.
    roots: Vec<String>,
    nodes: HashMap<String, Node>,
    /// Every POM read, by key: its text, or what happened instead.
    texts: HashMap<String, Result<String, String>>,
}

impl Walk {
    /// The walk of the project chain `seeded` (`(key, text)`, the project's
    /// POM first, then its parents nearest first), already read.
    pub(super) fn new(seeded: &[(String, &str)], unread: &mut Vec<String>) -> Self {
        let mut walk = Self {
            roots: Vec::new(),
            nodes: HashMap::new(),
            texts: seeded
                .iter()
                .map(|(key, xml)| (key.clone(), Ok((*xml).to_owned())))
                .collect(),
        };
        let chain = seeded.iter().map(|(_, xml)| *xml).collect::<Vec<_>>();
        walk.roots = walk.imports_of(&chain, unread);
        walk
    }

    /// The BOMs a chain's POMs import (resolved with the chain's effective
    /// properties, so a nearer POM's value wins), as new nodes; an import
    /// whose coordinate keeps a property no POM read defines is stated in
    /// `unread`.
    fn imports_of(&mut self, chain: &[&str], unread: &mut Vec<String>) -> Vec<String> {
        let mut keys = Vec::new();
        let properties = chain_properties(chain);
        for xml in chain {
            for import in imports(xml, &properties) {
                match import {
                    Ok((group, artifact, version)) => {
                        let bom = Parent {
                            group,
                            artifact,
                            version,
                        };
                        let key = bom.key();
                        self.nodes.entry(key.clone()).or_insert_with(|| Node {
                            next: Some(bom.clone()),
                            bom,
                            chain: Vec::new(),
                            imports: Vec::new(),
                            failed: false,
                        });
                        keys.push(key);
                    }
                    Err(named) => unread.push(format!(
                        "Imported BOM {named} names a property no POM read defines: a dependency scope it manages is unknown, not absent."
                    )),
                }
            }
        }
        keys
    }

    /// The BOMs in Maven's import order, each at its first place, as
    /// `(key, its chain's texts)`: `None` for one not read whole yet. One
    /// that failed to read manages nothing and is left out.
    pub(super) fn order(&self) -> Vec<(&str, Option<Vec<&str>>)> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut stack = self.roots.iter().rev().collect::<Vec<_>>();
        while let Some(key) = stack.pop() {
            let Some(node) = self.nodes.get(key) else {
                continue;
            };
            if !seen.insert(key) || node.failed {
                continue;
            }
            if node.next.is_some() {
                out.push((key.as_str(), None));
                continue;
            }
            let chain = node
                .chain
                .iter()
                .filter_map(|key| self.texts.get(key)?.as_deref().ok())
                .collect();
            out.push((key.as_str(), Some(chain)));
            stack.extend(node.imports.iter().rev());
        }
        out
    }

    /// Reads `targets` (BOMs not read whole) one POM further each, fetching
    /// every POM they need next together; one already read is not fetched
    /// again. A cancelled call fails the lookup.
    pub(super) async fn advance(
        &mut self,
        targets: &[String],
        unread: &mut Vec<String>,
        client: &RegistryClient<'_>,
    ) -> Result<(), ArtifactError> {
        self.settle(targets, unread);
        let mut wanted: Vec<Parent> = Vec::new();
        for next in targets
            .iter()
            .filter_map(|key| self.nodes.get(key)?.next.as_ref())
        {
            let key = next.key();
            if !self.texts.contains_key(&key) && !wanted.iter().any(|known| known.key() == key) {
                wanted.push(next.clone());
            }
        }
        let fetched = futures_util::future::join_all(wanted.iter().map(|pom| {
            let path = super::super::group_path(&pom.group);
            async move { fetch_pom(&path, &pom.artifact, &pom.version, client).await }
        }))
        .await;
        for (pom, fetched) in wanted.iter().zip(fetched) {
            let text = match fetched {
                Ok(Fetched::Pom(xml)) => Ok(xml),
                Ok(Fetched::Missing) => Err(MISSING.to_owned()),
                Ok(Fetched::Unsafe) => Err(super::UNSAFE.to_owned()),
                Err(error) if error.code == ErrorCode::Cancelled => return Err(error),
                Err(error) => Err(format!(
                    "could not be read ({})",
                    error.message.trim_end_matches('.')
                )),
            };
            self.texts.insert(pom.key(), text);
        }
        self.settle(targets, unread);
        Ok(())
    }

    /// Moves each target along the POMs already read, until it needs one
    /// that is not or its chain is whole.
    fn settle(&mut self, targets: &[String], unread: &mut Vec<String>) {
        for key in targets {
            loop {
                let chain = match self.step(key, unread) {
                    Step::Waiting => break,
                    Step::Moved => continue,
                    Step::Whole(chain) => chain,
                };
                let chain = chain.iter().map(String::as_str).collect::<Vec<_>>();
                let imports = self.imports_of(&chain, unread);
                if let Some(node) = self.nodes.get_mut(key) {
                    node.imports = imports;
                }
            }
        }
    }

    /// One POM along `key`'s chain.
    fn step(&mut self, key: &str, unread: &mut Vec<String>) -> Step {
        let Some(node) = self.nodes.get_mut(key) else {
            return Step::Waiting;
        };
        let Some(next) = node.next.clone() else {
            return Step::Waiting;
        };
        let next_key = next.key();
        let Some(text) = self.texts.get(&next_key) else {
            return Step::Waiting;
        };
        match text {
            Ok(xml) => {
                node.chain.push(next_key);
                node.next = located(xml)
                    .parent
                    .filter(|parent| !parent.version.contains("${"))
                    .filter(|parent| !node.chain.contains(&parent.key()));
            }
            Err(what) if node.chain.is_empty() => {
                unread.push(format!(
                    "Imported BOM {} {what}: a dependency scope it manages is unknown, not absent.",
                    next.named()
                ));
                node.failed = true;
                node.next = None;
                return Step::Moved;
            }
            Err(what) => {
                // Missing or not: what it would manage is unknown.
                unread.push(format!(
                    "Parent POM {} of imported BOM {} {what}: a dependency scope it manages is unknown, not absent.",
                    next.named(),
                    node.bom.named()
                ));
                node.next = None;
            }
        }
        if node.next.is_some() {
            return Step::Moved;
        }
        let chain = node
            .chain
            .iter()
            .filter_map(|key| self.texts.get(key)?.as_ref().ok().cloned())
            .collect();
        Step::Whole(chain)
    }
}

/// What one [`Walk::step`] did.
enum Step {
    /// Nothing: the next POM is not read yet, or the chain is whole.
    Waiting,
    /// Read one POM further.
    Moved,
    /// Made the chain whole: its texts, nearest first.
    Whole(Vec<String>),
}
