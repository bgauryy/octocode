/* -------------------------------- batch ----------------------------------- */
import { OqlInputBatchSchema } from '../schema.js';
import { OqlValidationError, diagnostic } from '../diagnostics.js';
import type { OqlBatch, OqlInputBatch, OqlInputQuery } from '../types.js';
import { fail, formatZodError } from './shared.js';
import { normalizeQuery } from './query.js';

export function normalizeBatch(input: OqlInputBatch): OqlBatch {
  const parsed = OqlInputBatchSchema.safeParse(input);
  if (!parsed.success) {
    fail(diagnostic('invalidQuery', formatZodError(parsed.error)));
  }
  const raw = parsed.data as OqlInputBatch;
  // Reject unknown batch-level keys (same strictness as query level — e.g.
  // `batchId` is not a field; the id field is `id`).
  const KNOWN_BATCH_KEYS = new Set([
    'schema',
    'id',
    'queries',
    'combine',
    'limit',
    'page',
    'itemsPerPage',
    'explain',
  ]);
  for (const key of Object.keys(raw)) {
    if (!KNOWN_BATCH_KEYS.has(key)) {
      fail(
        diagnostic(
          'unknownField',
          `Unknown batch field "${key}" is not part of OQL.`,
          { queryPath: key }
        )
      );
    }
  }
  if (raw.queries.length > 5) {
    fail(
      diagnostic(
        'invalidQuery',
        'OQL batches are capped at 5 queries per call.',
        { queryPath: 'queries' }
      )
    );
  }
  const queries = raw.queries.map((q, i) => {
    try {
      return normalizeQuery(q as OqlInputQuery);
    } catch (err) {
      if (err instanceof OqlValidationError) {
        // prefix queryPath with the child index for traceability
        throw new OqlValidationError(
          err.diagnostics.map(d => ({
            ...d,
            queryPath: `queries[${i}]${d.queryPath ? `.${d.queryPath}` : ''}`,
          }))
        );
      }
      throw err;
    }
  });
  return {
    schema: 'oql',
    ...(raw.id ? { id: raw.id } : {}),
    queries,
    combine: raw.combine ?? 'independent',
    ...(raw.limit !== undefined ? { limit: raw.limit } : {}),
    ...(raw.page !== undefined ? { page: raw.page } : {}),
    ...(raw.itemsPerPage !== undefined
      ? { itemsPerPage: raw.itemsPerPage }
      : {}),
    ...(raw.explain !== undefined ? { explain: raw.explain } : {}),
  };
}
