//! `const f = ns.g`: what a call of such a name calls. A `const` cannot be
//! rebound, so the binding says it for as long as its scope lasts.
use super::*;

impl ReceiverScopes {
    /// The declarators read next belong to a declaration of `kind`; returns the
    /// previous state for [`Self::leave_declaration`].
    pub(in crate::signatures) fn enter_declaration(
        &mut self,
        kind: VariableDeclarationKind,
    ) -> bool {
        std::mem::replace(&mut self.in_const, kind == VariableDeclarationKind::Const)
    }

    pub(in crate::signatures) fn leave_declaration(&mut self, previous: bool) {
        self.in_const = previous;
    }

    /// What `const name = a.b` makes a call of `name` mean: a `const` cannot be
    /// rebound, and the value is a plain name path, through any earlier alias.
    pub(super) fn const_alias(&self, it: &VariableDeclarator) -> Option<String> {
        if !self.in_const || !matches!(it.id, BindingPattern::BindingIdentifier(_)) {
            return None;
        }
        let value = dotted_expression_name(it.init.as_ref()?)?;
        let (head, rest) = value.split_once('.').unwrap_or((value.as_str(), ""));
        Some(match self.alias_of(head) {
            Some(alias) if rest.is_empty() => alias.to_owned(),
            Some(alias) => format!("{alias}.{rest}"),
            None => value,
        })
    }

    fn alias_of(&self, name: &str) -> Option<&str> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))?
            .alias
            .as_deref()
    }

    /// What a call of `callee` calls when it names a `const` alias.
    pub(in crate::signatures) fn call_alias(&self, callee: &Expression) -> Option<String> {
        let Expression::Identifier(name) = unwrap_expr(callee) else {
            return None;
        };
        self.alias_of(name.name.as_str()).map(str::to_owned)
    }
}
