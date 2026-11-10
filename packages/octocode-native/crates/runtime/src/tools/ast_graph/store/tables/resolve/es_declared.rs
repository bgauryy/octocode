//! TS member calls whose receiver a declared type reaches
//! ([`GraphFactCall::receiver_declared`](octocode_engine::graph::GraphFactCall)),
//! typed the way tsserver reads declared types, across files: `x.f.m()` with
//! `x: T` is the member `m` of the type `T` declares for `f`, `T["k"]` is the
//! type of `T`'s member `k`, and a parameter of `const f: A = (p) => …` has the
//! type of `A`'s parameter. Each name resolves in the scope of the file that
//! writes it, through that file's imports, and a member missing from a class
//! or interface is looked up on what it `extends`. Only declared types take
//! part: a member written without a type, or one that depends on a type
//! argument, types nothing. The cycle guard holds the alias and base
//! expansions in progress on one path: an alias or class met again on a later
//! step (`a.parent.scene` through an inherited `parent: Node`) expands again.
use super::linker::{Linked, Linker, Site};
use super::*;
use octocode_engine::graph::{GraphDeclaredType, GraphTypeStep};

/// The declared types of the graph's type declarations, and each file's link
/// context to resolve the names they write.
#[derive(Default)]
pub(crate) struct DeclaredTypes<'r> {
    pub(crate) types: HashMap<u32, &'r GraphDeclaredType>,
    pub(crate) units: HashMap<u32, &'r FileLinks<'r>>,
    /// The type declarations of the global scope (`declare global`, script
    /// files) by name: same-named interfaces merge, so a name holds them all.
    pub(crate) globals: HashMap<String, Vec<u32>>,
}

impl<'r> DeclaredTypes<'r> {
    /// The declared types of every file's type declarations, by node, and
    /// each file's link context, by file node.
    pub(crate) fn of(
        units: &'r [(FileLinks<'r>, &'r crate::tools::ast_graph::types::FileFacts)],
    ) -> Self {
        let types = units
            .iter()
            .flat_map(|(unit, facts)| {
                facts.declared_types.iter().filter_map(|declared| {
                    let node = unit
                        .index
                        .decl_nodes
                        .get(declared.declaration_id.as_str())?;
                    Some((*node, &declared.ty))
                })
            })
            .collect();
        let mut globals: HashMap<String, Vec<u32>> = HashMap::new();
        for (unit, facts) in units {
            for declared in facts
                .declared_types
                .iter()
                .filter(|declared| declared.global)
            {
                let id = declared.declaration_id.as_str();
                let Some(node) = unit.index.decl_nodes.get(id) else {
                    continue;
                };
                // `path#Outer.Name@line:col:kind`: the declared name.
                let local = id.split_once('#').map_or(id, |(_, local)| local);
                let local = local.split('@').next().unwrap_or(local);
                let name = local.rsplit('.').next().unwrap_or(local);
                globals.entry(name.to_owned()).or_default().push(*node);
            }
        }
        Self {
            globals,
            types,
            units: units.iter().map(|(unit, _)| (unit.file_id, unit)).collect(),
        }
    }
}

/// A type reached on the way: a declaration node, or a type written in a
/// file (whose node is `file`), at the confidence of the weakest link that
/// reached it.
#[derive(Clone, Copy)]
enum Reached<'a> {
    Node(u32, Confidence),
    Written(&'a GraphDeclaredType, u32, Confidence),
}

impl Linker<'_> {
    /// `site.name` on the type `declared` reaches from the caller's file.
    pub(super) fn declared_member(
        &self,
        site: &Site,
        declared: &GraphDeclaredType,
    ) -> Option<Linked> {
        let file = self.file_of(site.caller);
        let mut expanding = HashSet::new();
        let owners = self.settle(
            Reached::Written(declared, file, Confidence::High),
            &mut expanding,
        );
        let mut targets = BTreeSet::new();
        let mut weakest = Confidence::High;
        let mut inherited = false;
        for (owner, confidence) in owners {
            if let Some((members, via, strength)) =
                self.bound_member(site, owner, false, confidence, "receiver-type")
            {
                targets.extend(members);
                weakest = weakest.max(strength);
                inherited |= via == "inherited";
            }
        }
        let targets = targets.into_iter().collect::<Vec<_>>();
        let confidence = confidence_for(&targets, weakest);
        let via = if inherited {
            "inherited"
        } else {
            "receiver-type"
        };
        (!targets.is_empty()).then_some((targets, via, confidence))
    }

    /// The declaration nodes `reached` stands for: a named type resolves in
    /// its file, a type alias to the type it names; an object or function
    /// type written in place declares no members of its own. `expanding`
    /// holds the aliases and classes being expanded on this path, so only a
    /// cycle stops.
    fn settle<'a>(
        &'a self,
        reached: Reached<'a>,
        expanding: &mut HashSet<u32>,
    ) -> Vec<(u32, Confidence)> {
        match reached {
            Reached::Node(node, confidence) => match self.declared.types.get(&node) {
                Some(ty @ GraphDeclaredType::Named { .. }) if expanding.insert(node) => {
                    let written = Reached::Written(ty, self.file_of(node), confidence);
                    let settled = self.settle(written, expanding);
                    expanding.remove(&node);
                    settled
                }
                Some(GraphDeclaredType::Named { .. }) => Vec::new(),
                _ => vec![(node, confidence)],
            },
            Reached::Written(GraphDeclaredType::Named { name, steps }, file, confidence) => {
                // `f`'s parameter: the name may be a function, not a type.
                let callable = matches!(steps.first(), Some(GraphTypeStep::Parameter(_)));
                let mut reached = self.declared_named(file, name, confidence, callable);
                for step in steps {
                    reached = reached
                        .into_iter()
                        .flat_map(|item| self.step(item, step, expanding))
                        .collect();
                }
                reached
                    .into_iter()
                    .flat_map(|item| self.settle(item, expanding))
                    .collect()
            }
            Reached::Written(..) => Vec::new(),
        }
    }

    /// The global declarations of `name` merged across the repository. The
    /// declaring files may sit outside the program that reads them: `medium`.
    fn global_named(&self, name: &str, confidence: Confidence) -> Vec<Reached<'static>> {
        self.declared
            .globals
            .get(name)
            .into_iter()
            .flatten()
            .map(|node| Reached::Node(*node, confidence.max(Confidence::Medium)))
            .collect()
    }

    /// The type declarations `name` names in `file`'s scope.
    fn declared_named(
        &self,
        file: u32,
        name: &str,
        confidence: Confidence,
        callable: bool,
    ) -> Vec<Reached<'static>> {
        let Some(unit) = self.declared.units.get(&file) else {
            return Vec::new();
        };
        let (qualifier, name) = split_callee(name);
        // `globalThis.T`: only the merged global declarations of `T`.
        if qualifier == "globalThis" {
            return self.global_named(name, confidence);
        }
        let accept = |node: u32| {
            (self.is_type)(node)
                || callable
                    && matches!(
                        self.declared.types.get(&node),
                        Some(GraphDeclaredType::Function { .. })
                    )
        };
        // A name an import binds that did not link (an external package or an
        // unresolved module) is that import's: no same-named declaration of
        // the repository, global or not, is its type.
        let bindings = &unit.bindings;
        if qualifier.is_empty()
            && !bindings.named.contains_key(name)
            && (bindings.external.contains(name) || bindings.unknown.contains(name))
        {
            return Vec::new();
        }
        let site = Site {
            bindings: &unit.bindings,
            index: unit.index,
            caller: unit.file_id,
            qualifier,
            name,
            accept: &accept,
            receiver_type: None,
            term_binding: false,
            self_template: None,
            value_qualifier: false,
        };
        // A type the file neither declares nor imports is not a same-named
        // declaration a name match picks (`unique-name`, `same-package`,
        // `import-scope`): another module's declaration is invisible here
        // unless it is global, so only a scope or a merged global names it.
        let Some((targets, _, linked)) = self
            .link(&site)
            .filter(|(_, via, _)| !matches!(*via, "unique-name" | "same-package" | "import-scope"))
        else {
            // A name the file neither declares nor imports: a global
            // declaration merged across the repository. The declaring files
            // may sit outside the program that reads them: `medium`. An
            // import binds the name even when it reaches no declaration.
            let imported = bindings.strict_names.contains(name)
                || bindings.named.contains_key(name)
                || bindings.modules.contains_key(name);
            return if qualifier.is_empty() && !imported {
                self.global_named(name, confidence)
            } else {
                Vec::new()
            };
        };
        targets
            .into_iter()
            .map(|node| Reached::Node(node, confidence.max(linked)))
            .collect()
    }

    /// The types one `step` reaches from `from` (`expanding` as in
    /// [`Self::settle`]).
    fn step<'a>(
        &'a self,
        from: Reached<'a>,
        step: &GraphTypeStep,
        expanding: &mut HashSet<u32>,
    ) -> Vec<Reached<'a>> {
        match from {
            Reached::Node(node, confidence) => {
                let Some(ty) = self.declared.types.get(&node).copied() else {
                    return Vec::new();
                };
                let file = self.file_of(node);
                let own = self.step(Reached::Written(ty, file, confidence), step, expanding);
                match (step, ty) {
                    // A class or interface without the member: what it extends.
                    (GraphTypeStep::Member(name), GraphDeclaredType::Object { members })
                        if !members.iter().any(|member| member.name == *name)
                            && expanding.insert(node) =>
                    {
                        let inherited = self
                            .bases
                            .parents
                            .get(&node)
                            .into_iter()
                            .flatten()
                            .flat_map(|(base, edge)| {
                                self.step(
                                    Reached::Node(*base, confidence.max(*edge)),
                                    step,
                                    expanding,
                                )
                            })
                            .collect();
                        expanding.remove(&node);
                        inherited
                    }
                    _ => own,
                }
            }
            Reached::Written(ty, file, confidence) => match (ty, step) {
                (GraphDeclaredType::Named { .. }, _) => self
                    .settle(from, expanding)
                    .into_iter()
                    .flat_map(|(node, confidence)| {
                        self.step(Reached::Node(node, confidence), step, expanding)
                    })
                    .collect(),
                (GraphDeclaredType::Object { members }, GraphTypeStep::Member(name)) => members
                    .iter()
                    .find(|member| member.name == *name)
                    .and_then(|member| member.ty.as_ref())
                    .map(|ty| Reached::Written(ty, file, confidence))
                    .into_iter()
                    .collect(),
                (GraphDeclaredType::Function { parameters }, GraphTypeStep::Parameter(index)) => {
                    parameters
                        .get(*index as usize)
                        .and_then(Option::as_ref)
                        .map(|ty| Reached::Written(ty, file, confidence))
                        .into_iter()
                        .collect()
                }
                _ => Vec::new(),
            },
        }
    }
}
