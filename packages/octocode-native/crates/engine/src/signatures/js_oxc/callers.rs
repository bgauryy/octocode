//! Which declaration a JS/TS call belongs to.
//!
//! The call walk names an owner per call: a top-level declaration, a member
//! key or the `module` placeholder. Nested callables do not change that
//! owner during the walk. A nested function declaration, or a function or
//! arrow bound to a name, is still a declaration (a graph node). So a call
//! belongs to the innermost callable declaration whose range holds it. The
//! walk's owner applies only when no callable declaration lies inside it.

use crate::graph::{GraphFactCall, GraphFactDeclaration, GraphPosition};
use std::collections::HashMap;

/// Declaration kinds that run code: their calls are their own.
const CALLABLE_KINDS: &[&str] = &["function", "method", "constructor"];

/// Callable declarations by range start, each with its nearest enclosing
/// callable (ranges nest or are disjoint).
pub(super) struct Callers<'d> {
    by_name: HashMap<&'d str, Vec<&'d GraphFactDeclaration>>,
    callables: Vec<&'d GraphFactDeclaration>,
    parents: Vec<Option<usize>>,
}

fn holds(declaration: &GraphFactDeclaration, call: &GraphFactCall) -> bool {
    declaration.range.start <= call.range.start && declaration.range.end >= call.range.end
}

/// Whether `inner` lies within `outer`: a later start, or the same start
/// and an earlier end.
fn narrower(inner: &GraphFactDeclaration, outer: &GraphFactDeclaration) -> bool {
    (&inner.range.start, std::cmp::Reverse(&inner.range.end))
        > (&outer.range.start, std::cmp::Reverse(&outer.range.end))
}

impl<'d> Callers<'d> {
    pub(super) fn new(declarations: &'d [GraphFactDeclaration]) -> Self {
        let mut by_name: HashMap<&str, Vec<&GraphFactDeclaration>> = HashMap::new();
        for declaration in declarations {
            by_name
                .entry(declaration.name.as_str())
                .or_default()
                .push(declaration);
        }
        let mut callables = declarations
            .iter()
            .filter(|declaration| CALLABLE_KINDS.contains(&declaration.kind.as_str()))
            .collect::<Vec<_>>();
        callables.sort_by(|left, right| {
            (&left.range.start, std::cmp::Reverse(&left.range.end))
                .cmp(&(&right.range.start, std::cmp::Reverse(&right.range.end)))
        });
        let mut parents = Vec::with_capacity(callables.len());
        let mut open: Vec<usize> = Vec::new();
        for (index, callable) in callables.iter().enumerate() {
            while let Some(&last) = open.last() {
                if callables[last].range.end >= callable.range.end {
                    break;
                }
                open.pop();
            }
            parents.push(open.last().copied());
            open.push(index);
        }
        Self {
            by_name,
            callables,
            parents,
        }
    }

    /// The innermost callable declaration holding `call`: the last one
    /// starting at or before it, or that one's nearest holding ancestor.
    fn innermost_callable(&self, call: &GraphFactCall) -> Option<&'d GraphFactDeclaration> {
        let start: &GraphPosition = &call.range.start;
        let after = self
            .callables
            .partition_point(|callable| callable.range.start <= *start);
        let mut at = after.checked_sub(1);
        while let Some(index) = at {
            let callable = self.callables[index];
            if holds(callable, call) {
                return Some(callable);
            }
            at = self.parents[index];
        }
        None
    }

    /// The smallest declaration named by the walk's owner that holds `call`.
    fn named_owner(&self, call: &GraphFactCall) -> Option<&'d GraphFactDeclaration> {
        self.by_name
            .get(call.caller.as_str())
            .into_iter()
            .flatten()
            .copied()
            .filter(|declaration| holds(declaration, call))
            .reduce(|best, next| if narrower(next, best) { next } else { best })
    }

    /// The declaration `call` belongs to: the narrower of the walk's owner
    /// and the innermost callable holding it.
    pub(super) fn of(&self, call: &GraphFactCall) -> Option<&'d GraphFactDeclaration> {
        match (self.named_owner(call), self.innermost_callable(call)) {
            (Some(owner), Some(callable)) if narrower(callable, owner) => Some(callable),
            (Some(owner), _) => Some(owner),
            (None, callable) => callable,
        }
    }
}
