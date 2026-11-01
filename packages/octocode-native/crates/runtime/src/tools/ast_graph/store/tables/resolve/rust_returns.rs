//! Rust receivers typed by the call whose value they are, as rust-analyzer
//! types them: `T::open()?.m()` and `let x = c.f(); x.m()` see the declared
//! return type of the function that call links to (the type `?` yields from
//! a `Result`/`Option` when the value passes through `?`, or through an
//! unlinked `.unwrap()`/`.expect(..)` on a declared std `Result`/`Option`).
use super::linker::Linker;
use super::*;
use crate::tools::ast_graph::types::Call;
use octocode_engine::graph::GraphReceiverCall;

/// A receiver type derived from another call's link, and that link's
/// confidence: the member call it types is no surer.
pub(super) type Received = Option<(String, Confidence)>;

/// The derived receiver type of each of the file's calls, in call order;
/// `None` where the call has a written receiver type, no receiver call, or
/// the receiver call links to no single declared return type.
pub(super) fn received_types(
    linker: &Linker,
    unit: &FileLinks,
    facts: &FileFacts,
    callable: &dyn Fn(u32) -> bool,
    method: &dyn Fn(u32) -> bool,
) -> Vec<Received> {
    let calls = &facts.calls;
    if calls.iter().all(|call| call.receiver_call.is_none()) {
        return vec![None; calls.len()];
    }
    let mut receiving = Receiving {
        linker,
        unit,
        calls,
        callable,
        method,
        by_span: calls
            .iter()
            .enumerate()
            .filter_map(|(index, call)| {
                let span = call.source_range?.range;
                Some(((span.start_byte, span.end_byte), index))
            })
            .collect(),
        memo: vec![None; calls.len()],
    };
    (0..calls.len())
        .map(|index| receiving.received(index))
        .collect()
}

struct Receiving<'a, 'r> {
    linker: &'a Linker<'r>,
    unit: &'a FileLinks<'a>,
    calls: &'a [Call],
    callable: &'a dyn Fn(u32) -> bool,
    method: &'a dyn Fn(u32) -> bool,
    /// Call index by the bytes of its call expression.
    by_span: HashMap<(u32, u32), usize>,
    /// Settled derivations; a call being derived reads as `Some(None)`, so
    /// a malformed reference cycle ends unlinked.
    memo: Vec<Option<Received>>,
}

impl Receiving<'_, '_> {
    fn received(&mut self, index: usize) -> Received {
        if let Some(done) = &self.memo[index] {
            return done.clone();
        }
        self.memo[index] = Some(None);
        let derived = self.derive(index);
        self.memo[index] = Some(derived.clone());
        derived
    }

    fn derive(&mut self, index: usize) -> Received {
        let call = &self.calls[index];
        let reference = call
            .receiver_call
            .filter(|_| call.receiver_type.is_none())?;
        let inner = self.referenced(reference)?;
        match self.linked(inner) {
            Some((targets, confidence)) => {
                self.declared(&targets, confidence, |(returned, tried)| {
                    if reference.unwrap { tried } else { returned }
                })
            }
            None if !reference.unwrap => self.unwrapped(inner),
            None => None,
        }
    }

    /// The call whose expression spans `reference`.
    fn referenced(&self, reference: GraphReceiverCall) -> Option<usize> {
        self.by_span
            .get(&(reference.start_byte, reference.end_byte))
            .copied()
    }

    /// The targets the call at `index` links to with its written or derived
    /// receiver type, at the surer of the two links' confidences.
    fn linked(&mut self, index: usize) -> Option<(Vec<u32>, Confidence)> {
        let received = self.received(index);
        let call = &self.calls[index];
        let receiver_type = call
            .receiver_type
            .as_deref()
            .or(received.as_ref().map(|(ty, _)| ty.as_str()));
        let (_, linked) = resolve_call(
            self.linker,
            self.unit,
            call,
            receiver_type,
            self.callable,
            self.method,
        );
        let (targets, _, confidence) = linked.ok()?;
        Some((
            targets,
            received.map_or(confidence, |(_, inner)| confidence.max(inner)),
        ))
    }

    /// The one type every target declares through `pick`; candidates that
    /// disagree, or any target without one, leave the receiver untyped.
    fn declared<'t>(
        &self,
        targets: &[u32],
        confidence: Confidence,
        pick: impl Fn(ReturnTypes<'t>) -> Option<&'t str>,
    ) -> Received
    where
        Self: 't,
    {
        let mut types = targets
            .iter()
            .map(|id| self.linker.return_types.get(id).copied().and_then(&pick));
        let first = types.next()??;
        types
            .all(|ty| ty == Some(first))
            .then(|| (first.to_owned(), confidence))
    }

    /// The value an unlinked `.unwrap()`/`.expect(..)` at `index` yields: the
    /// `T` of its receiver's declared `Result<T, _>`/`Option<T>`, as `?`
    /// yields it. The receiver must be a call that links to declarations of
    /// that std wrapper, and no crate type of the wrapper's name may declare a
    /// method of that name; otherwise the method name proves nothing. The
    /// declared return type outranks a constructor-name `receiver_type`
    /// guess (`T::new(..)` declared `-> Result<Self, E>`).
    fn unwrapped(&mut self, index: usize) -> Received {
        let calls = self.calls;
        let call = &calls[index];
        let (_, name) = split_callee(&call.callee);
        if !matches!(name, "unwrap" | "expect") {
            return None;
        }
        let reference = call.receiver_call.filter(|reference| !reference.unwrap)?;
        let receiver = self.referenced(reference)?;
        let (targets, confidence) = self.linked(receiver)?;
        let members = self.linker.members;
        self.declared(&targets, confidence, |(returned, tried)| {
            let wrapper = returned?.rsplit("::").next()?;
            (matches!(wrapper, "Result" | "Option") && !members.contains_key(&(wrapper, name)))
                .then_some(tried?)
        })
    }
}
