//! TS member calls whose receiver a declared type reaches
//! ([`GraphFactCall::receiver_declared`](octocode_engine::graph::GraphFactCall)),
//! typed the way tsserver reads declared types, across files: `x.f.m()` with
//! `x: T` is the member `m` of the type `T` declares for `f`, `T["k"]` is the
//! type of `T`'s member `k`, and a parameter of `const f: A = (p) => …` has the
//! type of `A`'s parameter. Each name resolves in the scope of the file that
//! writes it, through that file's imports, and a member missing from a class
//! or interface is looked up on what it `extends`. Only declared types take
//! part: a member written without a type, or one that depends on a type
//! argument, types nothing.
use super::linker::{Linked, Linker, Site};
use super::*;
use octocode_engine::graph::{GraphDeclaredType, GraphTypeStep};

/// The declared types of the graph's type declarations, and each file's link
/// context to resolve the names they write.
#[derive(Default)]
pub(crate) struct DeclaredTypes<'r> {
    pub(crate) types: HashMap<u32, &'r GraphDeclaredType>,
    pub(crate) units: HashMap<u32, &'r FileLinks<'r>>,
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
        let mut seen = HashSet::new();
        let owners = self.settle(
            Reached::Written(declared, file, Confidence::High),
            &mut seen,
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
    /// type written in place declares no members of its own.
    fn settle<'a>(
        &'a self,
        reached: Reached<'a>,
        seen: &mut HashSet<u32>,
    ) -> Vec<(u32, Confidence)> {
        match reached {
            Reached::Node(node, confidence) => match self.declared.types.get(&node) {
                Some(ty @ GraphDeclaredType::Named { .. }) if seen.insert(node) => {
                    let written = Reached::Written(ty, self.file_of(node), confidence);
                    self.settle(written, seen)
                }
                Some(GraphDeclaredType::Named { .. }) => Vec::new(),
                _ => vec![(node, confidence)],
            },
            Reached::Written(GraphDeclaredType::Named { name, steps }, file, confidence) => {
                let mut reached = self.declared_named(file, name, confidence);
                for step in steps {
                    reached = reached
                        .into_iter()
                        .flat_map(|item| self.step(item, step, seen))
                        .collect();
                }
                reached
                    .into_iter()
                    .flat_map(|item| self.settle(item, seen))
                    .collect()
            }
            Reached::Written(..) => Vec::new(),
        }
    }

    /// The type declarations `name` names in `file`'s scope.
    fn declared_named(
        &self,
        file: u32,
        name: &str,
        confidence: Confidence,
    ) -> Vec<Reached<'static>> {
        let Some(unit) = self.declared.units.get(&file) else {
            return Vec::new();
        };
        let (qualifier, name) = split_callee(name);
        let site = Site {
            bindings: &unit.bindings,
            index: unit.index,
            caller: unit.file_id,
            qualifier,
            name,
            accept: self.is_type,
            receiver_type: None,
            term_binding: false,
            self_template: None,
            value_qualifier: false,
        };
        let Some((targets, _, linked)) = self.link(&site) else {
            return Vec::new();
        };
        targets
            .into_iter()
            .map(|node| Reached::Node(node, confidence.max(linked)))
            .collect()
    }

    /// The types one `step` reaches from `from`.
    fn step<'a>(
        &'a self,
        from: Reached<'a>,
        step: &GraphTypeStep,
        seen: &mut HashSet<u32>,
    ) -> Vec<Reached<'a>> {
        match from {
            Reached::Node(node, confidence) => {
                let Some(ty) = self.declared.types.get(&node).copied() else {
                    return Vec::new();
                };
                let file = self.file_of(node);
                let own = self.step(Reached::Written(ty, file, confidence), step, seen);
                match (step, ty) {
                    // A class or interface without the member: what it extends.
                    (GraphTypeStep::Member(name), GraphDeclaredType::Object { members })
                        if !members.iter().any(|member| member.name == *name)
                            && seen.insert(node) =>
                    {
                        self.bases
                            .parents
                            .get(&node)
                            .into_iter()
                            .flatten()
                            .flat_map(|(base, edge)| {
                                self.step(Reached::Node(*base, confidence.max(*edge)), step, seen)
                            })
                            .collect()
                    }
                    _ => own,
                }
            }
            Reached::Written(ty, file, confidence) => match (ty, step) {
                (GraphDeclaredType::Named { .. }, _) => self
                    .settle(from, seen)
                    .into_iter()
                    .flat_map(|(node, confidence)| {
                        self.step(Reached::Node(node, confidence), step, seen)
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
