//! Scala call and base-type linking, by Scala's static name binding.
//!
//! - A bare call `f()` binds the nearest enclosing scope that declares `f`,
//!   or a member that scope's template inherits ([`linearize`]); a scope
//!   with an unresolved base before the hit links nothing. Past the file's
//!   top level, what an import visible there brings in (an explicit
//!   selector, or a wildcard over an in-repo object or package), then the
//!   graph-wide fallback over top-level declarations: another file's member
//!   is never visible bare. A wildcard outside the scan stops both.
//! - `X.m()` names the object `X` (never the class `X`): a lexically visible
//!   object, one an explicit import names, one in the caller's directory, or
//!   the only top-level object of that name; a wildcard outside the scan
//!   that could supply `X` stops the last two.
//! - `x.m()` with a declared type binds the member through that type.
//! - `this.m()` binds the innermost template; `self.m()` through a self
//!   alias binds the template that declares the alias, which may enclose
//!   the caller's template.
//! - In a template with a typed self type (`self: T =>`) a name its own body
//!   declares may still be a member of `T`: such a call links nothing.
//! - A term the extractor saw bound in scope (`Call::term_binding`) links
//!   nothing.
//!
//! Same-named declarations in two build-variant directories (`scala-2.12`,
//! `scala-2.13+`) stay ambiguous.
use super::linker::{Linked, Linker, Site};
use super::*;

mod imports;
mod linearize;
mod scope;
use linearize::Lookup;
pub(crate) use linearize::{Linearizations, ScalaBase};
use scope::{unique, upper};

impl Linker<'_> {
    pub(crate) fn scala_call(&self, site: &Site) -> Option<Linked> {
        if site.name.is_empty() || site.term_binding {
            return None;
        }
        if let Some(ty) = site.receiver_type.filter(|_| !site.receiver()) {
            let (owner, _, confidence) = self.scala_type(site, ty, Some(site.caller))?;
            return self.scala_member(site, owner, false, confidence, "receiver-type");
        }
        if let Some(name) = site.self_template {
            let template = self.alias_template(site.caller, name)?;
            return self.scala_member(site, template, false, Confidence::High, "local");
        }
        match site.qualifier {
            "" => self.scala_bare(site),
            "this" | "super" => {
                let template = self
                    .chain(Some(site.caller))
                    .find(|id| self.is_template(*id))?;
                let skip = site.qualifier == "super";
                self.scala_member(site, template, skip, Confidence::High, "local")
            }
            qualifier => {
                let (module, confidence) = self.scala_object(site, qualifier)?;
                self.scala_member(site, module, false, confidence, "type-qualified")
            }
        }
    }

    /// A base type clause of the declaration `site.caller`, resolved in the
    /// scope that encloses the declaration. Aliases stay unresolved.
    pub(crate) fn scala_base(&self, site: &Site, written: &str) -> Option<Linked> {
        if written.starts_with('<') {
            return None;
        }
        let start = Some(self.nodes[site.caller as usize].parent).filter(|id| *id != NONE);
        let (base, via, confidence) = self.scala_type(site, written, start)?;
        (self.symbol_kind(base) != "type").then(|| (vec![base], via, confidence))
    }

    /// The one enclosing template named `name`: the declarer of a self alias.
    fn alias_template(&self, caller: u32, name: &str) -> Option<u32> {
        let templates = self
            .chain(Some(caller))
            .filter(|id| {
                self.is_template(*id)
                    && self.strings[self.nodes[*id as usize].name as usize] == name
            })
            .collect::<Vec<_>>();
        unique(&templates)
    }

    fn scala_bare(&self, site: &Site) -> Option<Linked> {
        for scope in self.scopes(site.caller) {
            let declared = self.declared_by(scope, site.caller, site.name);
            if !declared.is_empty() {
                if scope.is_some_and(|id| self.self_typed(id)) {
                    return None;
                }
                let targets = site.accepted(declared);
                let confidence = confidence_for(&targets, Confidence::High);
                return (!targets.is_empty()).then_some((targets, "local", confidence));
            }
            if let Some(template) = scope.filter(|id| self.is_template(*id)) {
                match self.linearized(template, site.name, site.accept, true) {
                    Lookup::Hit(targets, weakest, _) => {
                        let confidence = confidence_for(&targets, weakest);
                        return Some((targets, "inherited", confidence));
                    }
                    Lookup::Blocked => return None,
                    Lookup::Clear => {}
                }
            }
        }
        match self.scala_imported(site) {
            Ok(Some(linked)) => Some(linked),
            Ok(None) => {
                // Another file's member is visible only through an import:
                // the graph-wide fallback considers top-level declarations.
                let top_level =
                    |id: u32| (site.accept)(id) && self.nodes[id as usize].parent == NONE;
                self.global(
                    &Site {
                        accept: &top_level,
                        ..*site
                    },
                    "",
                )
            }
            Err(()) => None,
        }
    }

    fn scala_member(
        &self,
        site: &Site,
        owner: u32,
        skip_owner: bool,
        strong: Confidence,
        via: &'static str,
    ) -> Option<Linked> {
        match self.linearized(owner, site.name, site.accept, skip_owner) {
            Lookup::Hit(targets, weakest, inherited) => {
                let confidence = confidence_for(&targets, strong.max(weakest));
                Some((
                    targets,
                    if inherited { "inherited" } else { via },
                    confidence,
                ))
            }
            Lookup::Blocked | Lookup::Clear => None,
        }
    }

    /// The object a term path (`X`, `Outer.X`, `cats.data.X`) names.
    fn scala_object(&self, site: &Site, qualifier: &str) -> Option<(u32, Confidence)> {
        let segments = qualifier.split('.').collect::<Vec<_>>();
        let first = segments.iter().position(|s| upper(s))?;
        if segments.iter().any(|s| s.is_empty()) {
            return None;
        }
        let (package, objects) = segments.split_at(first);
        let (mut module, confidence) = if package.is_empty() {
            self.object_named(site, objects[0])?
        } else {
            let found = self.packaged(site, package, objects[0], &|id| self.is_module(id));
            (unique(&found)?, Confidence::High)
        };
        for name in &objects[1..] {
            let nested = self.declared_by(Some(module), module, name);
            module = unique(
                &nested
                    .into_iter()
                    .filter(|id| self.is_module(*id))
                    .collect::<Vec<_>>(),
            )?;
        }
        Some((module, confidence))
    }

    /// The object `X` in term position: a lexically visible object (a term
    /// of the same name in a nearer scope shadows it), else one an explicit
    /// import names, one in the caller's directory, or the only one.
    fn object_named(&self, site: &Site, name: &str) -> Option<(u32, Confidence)> {
        for scope in self.scopes(site.caller) {
            let declared = self.declared_by(scope, site.caller, name);
            let modules = declared
                .iter()
                .copied()
                .filter(|id| self.is_module(*id))
                .collect::<Vec<_>>();
            if !modules.is_empty() {
                return Some((unique(&modules)?, Confidence::High));
            }
            if declared.iter().any(|id| !(self.is_type)(*id))
                || scope.is_some_and(|id| self.is_template(id) && self.inherits_term(id, name))
            {
                return None;
            }
        }
        self.imported_or_near(site, name, &|id| self.is_module(id), true)
            .map(|(id, confidence, _)| (id, confidence))
    }

    /// A type name (`Monad`, `FlatMap.AbstractFlatMap`, `cats.Monad`) seen
    /// from `start` (the caller's scope chain from that node).
    fn scala_type(
        &self,
        site: &Site,
        written: &str,
        start: Option<u32>,
    ) -> Option<(u32, &'static str, Confidence)> {
        let is_type = |id: u32| (self.is_type)(id);
        match written.rsplit_once('.') {
            None => {
                for scope in self.chain(start).map(Some).chain([None]) {
                    let types = self
                        .declared_by(scope, site.caller, written)
                        .into_iter()
                        .filter(|id| is_type(*id))
                        .collect::<Vec<_>>();
                    if !types.is_empty() {
                        return Some((unique(&types)?, "local", Confidence::High));
                    }
                }
                let (id, confidence, via) =
                    self.imported_or_near(site, written, &is_type, false)?;
                Some((id, via, confidence))
            }
            Some((qualifier, name)) => {
                let (module, confidence) = self.scala_object(site, qualifier)?;
                let types = self
                    .declared_by(Some(module), module, name)
                    .into_iter()
                    .filter(|id| is_type(*id))
                    .collect::<Vec<_>>();
                Some((unique(&types)?, "type-qualified", confidence))
            }
        }
    }
}
