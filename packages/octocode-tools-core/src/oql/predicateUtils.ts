import type { Predicate } from './types.js';

/** Counts all/any/not nodes plus their leaves — used for the boolean-expansion budget and plan diagnostics. */
export function countPredicateNodes(p: Predicate | undefined): number {
  if (!p) return 0;
  if (p.kind === 'all' || p.kind === 'any') {
    return 1 + p.of.reduce((n, c) => n + countPredicateNodes(c), 0);
  }
  if (p.kind === 'not') {
    return 1 + countPredicateNodes(p.predicate);
  }
  return 1;
}
