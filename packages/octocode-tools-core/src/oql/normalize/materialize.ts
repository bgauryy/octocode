/* ---------------------------- materialize ------------------------------- */
import type {
  MaterializePolicy,
  OqlInputQuery,
  OqlQuery,
  Predicate,
  QuerySource,
} from '../types.js';

function isLocalOnlyPredicate(p: Predicate | undefined): boolean {
  if (!p) return false;
  switch (p.kind) {
    case 'structural':
      return true;
    case 'regex':
      return p.dialect === 'pcre2';
    case 'all':
    case 'any':
      // A multi-leaf boolean cannot be a single provider call, so it needs a
      // local/materialized corpus (set-algebra over per-leaf results).
      return true;
    case 'not':
      return isLocalOnlyPredicate(p.predicate);
    default:
      return false;
  }
}

export function normalizeMaterialize(
  raw: OqlInputQuery,
  from: QuerySource | undefined,
  where: Predicate | undefined,
  target: OqlQuery['target']
): MaterializePolicy | undefined {
  let policy: MaterializePolicy | undefined;
  if (typeof raw.materialize === 'string') {
    policy = { mode: raw.materialize };
  } else if (raw.materialize && typeof raw.materialize === 'object') {
    policy = raw.materialize as MaterializePolicy;
  }

  // target:"materialize" IS a clone op: it must materialize. Force mode away
  // from "never" and default to a bounded subtree so the planner's bounded-scope
  // safety check applies (an unbounded subtree without scope.path is refused).
  if (target === 'materialize' && from?.kind === 'github') {
    if (!policy) return { mode: 'required', strategy: 'subtree' };
    return {
      ...policy,
      mode: policy.mode === 'never' ? 'required' : policy.mode,
      strategy: policy.strategy ?? 'subtree',
    };
  }

  // Remote semantics has no provider-only lane: the adapter sparsely
  // materializes the requested file/repo, then runs LSP locally. Normalize that
  // internal route explicitly so `--explain` never says provider-only while
  // listing ghCloneRepo + lspGetSemantics backend calls.
  if (target === 'semantics' && from?.kind === 'github') {
    return {
      ...(policy ?? {}),
      mode: 'required',
      strategy: 'file',
    };
  }

  if (from?.kind !== 'github') {
    // local/materialized/npm/no-corpus sources don't need a materialize policy
    return policy;
  }

  if (policy) return policy;

  // GitHub source, no explicit policy: a local-only predicate (structural /
  // PCRE2) cannot be proven by the provider, so default to bounded
  // remote-as-local. Provider-capable predicates default to provider-only.
  if (isLocalOnlyPredicate(where)) {
    return { mode: 'auto', strategy: 'subtree' };
  }
  return { mode: 'never' };
}
