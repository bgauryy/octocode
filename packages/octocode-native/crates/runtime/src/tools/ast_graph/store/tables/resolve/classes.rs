//! Java and C# call and base-type linking, by their static name binding.
//!
//! - A simple method name `m()` binds the innermost enclosing declaration
//!   that declares `m`, or a member its class inherits: the superclass chain
//!   first, then (Java) interface default methods. C# partial parts are one
//!   class. An unresolved base before the hit could declare `m`, and every
//!   class inherits `Object`'s members: both link nothing past them. Then a
//!   static import (`import static C.m`, `import static C.*`, C#
//!   `using static C`), weakened when a base was unresolved. Never another
//!   class of the package or file: Java and C# have no free functions.
//! - `this.m()` binds the innermost class, `super.m()` (C#: `base.m()`) its
//!   bases; `x.m()` with a declared type binds through that type; `T.m()`
//!   through the type `T` (see [`types`]), never a field `T.f.m()`.
//! - `new T()` binds the type `T`.
use super::linker::{Linked, Linker, Site};
use super::*;

mod types;
pub(crate) use types::Namespaces;

/// A language whose classes this module links.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClassLang {
    Java,
    CSharp,
}

impl ClassLang {
    pub(crate) fn of(language: &str) -> Option<Self> {
        match language {
            "java" => Some(Self::Java),
            "csharp" => Some(Self::CSharp),
            _ => None,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Java => ".java",
            Self::CSharp => ".cs",
        }
    }

    /// A Java package is its directory here, so a package member stays a
    /// candidate; a C# namespace is declared.
    fn package_confidence(self) -> Confidence {
        match self {
            Self::Java => Confidence::Medium,
            Self::CSharp => Confidence::High,
        }
    }

    fn package_separator(self) -> &'static str {
        match self {
            Self::Java => "/",
            Self::CSharp => ".",
        }
    }

    /// Members every class inherits from the root object type.
    fn universal(self) -> &'static [&'static str] {
        match self {
            Self::Java => &[
                "equals",
                "hashCode",
                "toString",
                "getClass",
                "notify",
                "notifyAll",
                "wait",
                "clone",
                "finalize",
            ],
            Self::CSharp => &[
                "Equals",
                "GetHashCode",
                "ToString",
                "GetType",
                "MemberwiseClone",
                "ReferenceEquals",
                "Finalize",
            ],
        }
    }
}

/// What member lookup through a class and its bases found.
enum Walk {
    /// The first declaring classes' members, the weakest heritage confidence
    /// on the way, and whether a base (not the class itself) declared them.
    Hit(Vec<u32>, Confidence, bool),
    /// An unresolved base could declare the name.
    Blocked,
    /// Every base is known and none declares the name.
    Clear,
}

impl Linker<'_> {
    pub(crate) fn class_call(
        &self,
        site: &Site,
        lang: ClassLang,
        constructed: bool,
    ) -> Option<Linked> {
        if site.name.is_empty() {
            return None;
        }
        if constructed {
            let written = match site.qualifier {
                "" | "global" => site.name.to_owned(),
                qualifier => format!("{qualifier}.{}", site.name),
            };
            let (owners, via, confidence) =
                self.class_type(site, lang, &written, Some(site.caller))?;
            let owners = owners
                .into_iter()
                .filter(|id| (self.is_type)(*id))
                .collect::<Vec<_>>();
            let confidence = confidence_for(&owners, confidence);
            return (!owners.is_empty()).then_some((owners, via, confidence));
        }
        if let Some(ty) = site.receiver_type.filter(|_| !site.receiver()) {
            let (owners, _, confidence) = self.class_type(site, lang, ty, Some(site.caller))?;
            return self.class_member(site, lang, &owners, false, confidence, "receiver-type");
        }
        match (lang, site.qualifier) {
            (_, "") => self.class_bare(site, lang),
            (_, "this") | (ClassLang::Java, "super") | (ClassLang::CSharp, "base") => {
                let owner = self
                    .chain(Some(site.caller))
                    .find(|id| self.class_like(*id))?;
                let skip = site.qualifier != "this";
                self.class_member(
                    site,
                    lang,
                    &self.class_parts(owner),
                    skip,
                    Confidence::High,
                    "local",
                )
            }
            (_, qualifier) => {
                let (owners, _, confidence) =
                    self.class_type(site, lang, qualifier, Some(site.caller))?;
                self.class_member(site, lang, &owners, false, confidence, "type-qualified")
            }
        }
    }

    /// A base type clause of the declaration `site.caller`, resolved in the
    /// scope that encloses the declaration.
    pub(crate) fn class_base(&self, site: &Site, lang: ClassLang, written: &str) -> Option<Linked> {
        let start = Some(self.nodes[site.caller as usize].parent).filter(|id| *id != NONE);
        let (owners, via, confidence) = self.class_type(site, lang, written, start)?;
        let owners = owners
            .into_iter()
            .filter(|id| (self.is_type)(*id) && !self.class_parts(site.caller).contains(id))
            .collect::<Vec<_>>();
        (!owners.is_empty()).then(|| {
            let confidence = confidence_for(&owners, confidence);
            (owners, via, confidence)
        })
    }

    fn class_bare(&self, site: &Site, lang: ClassLang) -> Option<Linked> {
        let mut blocked = false;
        for scope in self.chain(Some(site.caller)) {
            let class = self.class_like(scope);
            let parts = if class {
                self.class_parts(scope)
            } else {
                vec![scope]
            };
            let declared = site.accepted(
                parts
                    .iter()
                    .flat_map(|part| self.declared_by(Some(*part), *part, site.name)),
            );
            if !declared.is_empty() {
                let confidence = confidence_for(&declared, Confidence::High);
                return Some((declared, "local", confidence));
            }
            if !class {
                continue;
            }
            match self.class_walk(site, lang, &parts, true) {
                Walk::Hit(targets, weakest, _) => {
                    let confidence = confidence_for(&targets, weakest);
                    return Some((targets, "inherited", confidence));
                }
                Walk::Blocked => {
                    blocked = true;
                    break;
                }
                Walk::Clear if lang.universal().contains(&site.name) => return None,
                Walk::Clear => {}
            }
        }
        let (targets, via, confidence) = self.static_import(site, lang)?;
        let confidence = if blocked {
            confidence.max(Confidence::Medium)
        } else {
            confidence
        };
        Some((targets, via, confidence))
    }

    /// What a static import (C#: `using static`) binds `site.name` to:
    /// single imports before on-demand ones.
    fn static_import(&self, site: &Site, lang: ClassLang) -> Option<Linked> {
        let mut single = Vec::new();
        let mut demand = Vec::new();
        for import in &site.bindings.written_imports {
            let (path, member) = match lang {
                ClassLang::Java => {
                    let Some(path) = import.specifier.strip_prefix("static ") else {
                        continue;
                    };
                    match path.trim().rsplit_once('.') {
                        Some((path, "*")) => (path, None),
                        Some((path, member)) if member == site.name => (path, Some(member)),
                        _ => continue,
                    }
                }
                ClassLang::CSharp if import.alias.is_none() => (import.specifier, None),
                ClassLang::CSharp => continue,
            };
            let segments = path.split('.').collect::<Vec<_>>();
            let Some(owners) = self.qualified_type(site, lang, &segments) else {
                continue;
            };
            if let Walk::Hit(targets, weakest, _) = self.class_walk(site, lang, &owners, false) {
                let list = if member.is_some() {
                    &mut single
                } else {
                    &mut demand
                };
                list.push((targets, weakest));
            }
        }
        let (found, via) = if single.is_empty() {
            let via = match lang {
                ClassLang::Java => "import-glob",
                ClassLang::CSharp => "import",
            };
            (demand, via)
        } else {
            (single, "import")
        };
        let mut targets = found
            .iter()
            .flat_map(|(targets, _)| targets.iter().copied())
            .collect::<Vec<_>>();
        targets.sort_unstable();
        targets.dedup();
        let weakest = found.iter().map(|(_, c)| *c).max()?;
        let confidence = confidence_for(&targets, weakest);
        Some((targets, via, confidence))
    }

    fn class_member(
        &self,
        site: &Site,
        lang: ClassLang,
        owners: &[u32],
        skip_owner: bool,
        strong: Confidence,
        via: &'static str,
    ) -> Option<Linked> {
        match self.class_walk(site, lang, owners, skip_owner) {
            Walk::Hit(targets, weakest, inherited) => {
                let confidence = confidence_for(&targets, strong.max(weakest));
                Some((
                    targets,
                    if inherited { "inherited" } else { via },
                    confidence,
                ))
            }
            Walk::Blocked | Walk::Clear => None,
        }
    }

    /// Member lookup from `owners` (the parts of one class): the superclass
    /// chain, then (Java) the interfaces of every class on it.
    fn class_walk(&self, site: &Site, lang: ClassLang, owners: &[u32], skip_owner: bool) -> Walk {
        let mut visited = HashSet::new();
        let mut classes = Vec::new();
        let start = owners
            .iter()
            .map(|owner| (*owner, Confidence::High, false))
            .collect::<Vec<_>>();
        let (found, inherited, blocked) =
            self.climb(site, start, skip_owner, &mut visited, &mut classes);
        if !found.is_empty() {
            return hit(found, inherited);
        }
        if blocked {
            return Walk::Blocked;
        }
        if lang == ClassLang::Java {
            let interfaces = classes
                .iter()
                .flat_map(|class| self.bases.interfaces.get(class).into_iter().flatten())
                .map(|(id, confidence)| (*id, *confidence, true))
                .collect::<Vec<_>>();
            let (found, _, blocked) =
                self.climb(site, interfaces, false, &mut visited, &mut classes);
            if !found.is_empty() {
                return hit(found, true);
            }
            if blocked {
                return Walk::Blocked;
            }
        }
        Walk::Clear
    }

    /// Depth-first through `extends` bases from `pending`, stopping each
    /// branch at the first class that declares the name; collects the
    /// classes visited.
    fn climb(
        &self,
        site: &Site,
        mut pending: Vec<(u32, Confidence, bool)>,
        skip_first: bool,
        visited: &mut HashSet<u32>,
        classes: &mut Vec<u32>,
    ) -> (BTreeMap<u32, Confidence>, bool, bool) {
        let mut found = BTreeMap::new();
        let (mut inherited, mut blocked) = (false, false);
        while let Some((current, confidence, through_base)) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }
            classes.push(current);
            let skip = skip_first && !through_base;
            let declared = if skip {
                Vec::new()
            } else {
                site.accepted(self.declared_by(Some(current), current, site.name))
            };
            if !declared.is_empty() {
                for id in declared {
                    found.insert(id, confidence);
                }
                inherited |= through_base;
                continue;
            }
            blocked |= self.bases.unresolved.contains(&current);
            for (base, edge) in self.bases.parents.get(&current).into_iter().flatten() {
                for part in self.class_parts(*base) {
                    pending.push((part, confidence.max(*edge), true));
                }
            }
        }
        (found, inherited, blocked)
    }
}

fn hit(found: BTreeMap<u32, Confidence>, inherited: bool) -> Walk {
    let weakest = found.values().copied().max().unwrap_or(Confidence::High);
    Walk::Hit(found.into_keys().collect(), weakest, inherited)
}
