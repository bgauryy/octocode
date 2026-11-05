// `classification.longContext` is a request to the provider: a marked header when
// true, none when false. The mock's top score keeps every hit, so evidence stays whole.
const LONG_CONTEXT_MARK = 'long-context';

/** A predicate unique to the row, so no earlier judgment answers from cache. */
export const uniquePredicate = assignment => `does this line belong to ${JSON.stringify(assignment)}`;

export function classificationBase(row, assignment) {
  return row.classification ? { classification: { predicates: [uniquePredicate(assignment)] } } : {};
}

/** What the provider heard about long context since the last look. */
export function longContextProblem(ctx, query, hasEvidence = true) {
  const wanted = query.classification?.longContext;
  const heard = ctx.providerHeaders.splice(0);
  if (!query.classification) return null;
  if (!heard.length) return hasEvidence ? 'classification row reached no provider' : null;
  const marked = heard.map(headers => JSON.stringify(headers).includes(LONG_CONTEXT_MARK));
  return wanted === true && !marked.every(Boolean) ? 'longContext true: provider request lacks the long-context mark'
    : wanted === false && marked.some(Boolean) ? 'longContext false: provider request carries the long-context mark' : null;
}
