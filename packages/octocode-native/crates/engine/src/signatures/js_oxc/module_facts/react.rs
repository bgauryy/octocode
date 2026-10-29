//! React wrappers that keep a component's identity: `memo(X)` and
//! `forwardRef(X)` render `X`, so `export default memo(X)` exports `X`.

use oxc_ast::ast::{Argument, Expression, ImportDeclarationSpecifier, Program, Statement};

/// The React wrappers admitted, by their imported names.
const WRAPPERS: &[&str] = &["memo", "forwardRef"];

/// Local names bound from the `react` module.
#[derive(Default)]
pub(super) struct ReactNames<'a> {
    /// Default and namespace imports (`React` in `React.memo`).
    namespaces: Vec<&'a str>,
    /// Named imports of a wrapper, by local name.
    wrappers: Vec<&'a str>,
}

impl<'a> ReactNames<'a> {
    pub(super) fn of(program: &'a Program<'a>) -> Self {
        let mut names = Self::default();
        for statement in &program.body {
            let Statement::ImportDeclaration(import) = statement else {
                continue;
            };
            if import.source.value != "react" || import.import_kind.is_type() {
                continue;
            }
            for specifier in import.specifiers.iter().flatten() {
                match specifier {
                    ImportDeclarationSpecifier::ImportSpecifier(named) => {
                        if !named.import_kind.is_type()
                            && WRAPPERS.contains(&named.imported.name().as_str())
                        {
                            names.wrappers.push(named.local.name.as_str());
                        }
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                        names.namespaces.push(default.local.name.as_str());
                    }
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                        names.namespaces.push(namespace.local.name.as_str());
                    }
                }
            }
        }
        names
    }

    /// The identifier a chain of React wrappers holds: `memo(X)`,
    /// `React.memo(X, eq)`, `memo(forwardRef(X))` → `X`.
    pub(super) fn wrapped(&self, expression: &'a Expression<'a>) -> Option<&'a str> {
        let Expression::CallExpression(call) = expression.without_parentheses() else {
            return None;
        };
        let wrapper = match call.callee.without_parentheses() {
            Expression::Identifier(callee) => self.wrappers.contains(&callee.name.as_str()),
            Expression::StaticMemberExpression(member) => {
                matches!(member.object.without_parentheses(),
                    Expression::Identifier(object) if self.namespaces.contains(&object.name.as_str()))
                    && WRAPPERS.contains(&member.property.name.as_str())
            }
            _ => false,
        };
        if !wrapper || call.optional {
            return None;
        }
        match call.arguments.first()? {
            Argument::Identifier(identifier) => Some(identifier.name.as_str()),
            Argument::SpreadElement(_) => None,
            argument => self.wrapped(argument.as_expression()?),
        }
    }
}
