//! The scope of a JS/TS local binding, from oxc semantics: a parameter, or a
//! name bound inside a function or a block, which no text outside that scope
//! and no other file can name.

use oxc_ast::AstKind;
use oxc_ast::ast::BindingPattern;
use oxc_semantic::{ScopeFlags, ScopeId, Scoping, Semantic, SemanticBuilder, SymbolId};
use oxc_span::GetSpan;

/// The byte range of the scope of the local binding that `name` at byte `at`
/// of `content` names (its declaration or a resolved use): the innermost
/// function (or class static block) around the binding, else the block that
/// binds it. `None` when the name there binds at module or script scope,
/// names a namespace member or an enum member (reached through their owner),
/// is a shorthand object-pattern element (the language server also searches
/// the property it reads), lies in a scope `with` or a direct `eval` can
/// extend, or the parse recovered from errors.
pub(crate) fn local_binding_scope(
    content: &str,
    file_path: &str,
    name: &str,
    at: usize,
) -> Option<std::ops::Range<usize>> {
    let (name, at) = (name.to_owned(), u32::try_from(at).ok()?);
    super::on_oxc_worker(content, file_path, None, move |content, file_path| {
        super::with_js_program::<false, _>(content, file_path, None, |_, parser_ret| {
            if !parser_ret.diagnostics.is_empty() {
                return None;
            }
            let semantic = SemanticBuilder::new()
                .with_build_nodes(true)
                .build(&parser_ret.program)
                .semantic;
            if super::super::js_oxc_arguments::dynamic_scope(&semantic) {
                return None;
            }
            let symbol = symbol_at(&semantic, &name, at)?;
            let span = local_scope(&semantic, symbol)?;
            Some(span.start as usize..span.end as usize)
        })
    })
}

/// The symbol `name` names at byte `at`: the binding declared there, or the
/// one the reference there resolves to.
fn symbol_at(semantic: &Semantic<'_>, name: &str, at: u32) -> Option<SymbolId> {
    let scoping = semantic.scoping();
    semantic.nodes().iter().find_map(|node| {
        let span = node.kind().span();
        if !(span.start..span.end).contains(&at) {
            return None;
        }
        match node.kind() {
            AstKind::BindingIdentifier(binding) if binding.name.as_str() == name => {
                binding.symbol_id.get()
            }
            AstKind::IdentifierReference(reference) if reference.name.as_str() == name => reference
                .reference_id
                .get()
                .and_then(|id| scoping.get_reference(id).symbol_id()),
            _ => None,
        }
    })
}

/// The span of the scope `symbol` is local to; `None` when another file can
/// name it (see [`local_binding_scope`]).
fn local_scope(semantic: &Semantic<'_>, symbol: SymbolId) -> Option<oxc_span::Span> {
    let scoping = semantic.scoping();
    if scoping.symbol_flags(symbol).is_enum_member() || binds_a_property(semantic, symbol) {
        return None;
    }
    let own = scoping.symbol_scope_id(symbol);
    if own == scoping.root_scope_id() {
        return None;
    }
    let mut boundary = None;
    for scope in scoping.scope_ancestors(own) {
        let flags = scoping.scope_flags(scope);
        if flags.is_ts_module_block() {
            // A namespace member may be exported through its namespace.
            return None;
        }
        if flags.is_function() || flags.contains(ScopeFlags::ClassStaticBlock) {
            boundary = Some(scope);
            break;
        }
    }
    let scope = boundary.unwrap_or(own);
    Some(node_span(semantic, scoping, scope))
}

fn node_span(semantic: &Semantic<'_>, scoping: &Scoping, scope: ScopeId) -> oxc_span::Span {
    semantic.nodes().kind(scoping.get_node_id(scope)).span()
}

/// Whether `symbol` is bound by a shorthand object-pattern element
/// (`function f({ core })`, `const { core = 1 } = o`): the name is a
/// property key too, so its uses are not bounded by its scope.
fn binds_a_property(semantic: &Semantic<'_>, symbol: SymbolId) -> bool {
    let span = semantic.scoping().symbol_span(symbol);
    semantic.nodes().iter().any(|node| {
        let AstKind::BindingProperty(property) = node.kind() else {
            return false;
        };
        let bound = match &property.value {
            BindingPattern::BindingIdentifier(binding) => Some(binding.span),
            BindingPattern::AssignmentPattern(assignment) => match &assignment.left {
                BindingPattern::BindingIdentifier(binding) => Some(binding.span),
                _ => None,
            },
            _ => None,
        };
        property.shorthand && bound == Some(span)
    })
}
