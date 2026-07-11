/**
 * OQL normalizer: raw sugar in, strict canonical OQL out.
 *
 * Rules (see OCTOCODE_QUERY_LANGUAGE.md §normalization):
 *  - sugar is accepted only when it has a deterministic rewrite;
 *  - ambiguous sugar fails with `ambiguousSugar`;
 *  - reserved targets fail with `unsupportedTarget`;
 *  - unknown fields fail with `unknownField`;
 *  - canonical output contains no shorthand fields.
 *
 * Predicate IDs are NOT injected here — the planner derives stable IDs from
 * node position (or a user-provided `id`) so the canonical `where` stays clean.
 *
 * This file is a thin barrel/orchestrator: the actual per-clause
 * normalization logic lives under ./normalize/* (source, scope, where,
 * materialize, params, query orchestration, batch handling).
 */
import type {
  OqlCanonicalInput,
  OqlInputQuery,
  OqlSearchInput,
} from './types.js';
import { isBatchInput } from './types.js';
import { normalizeBatch } from './normalize/batch.js';
import { normalizeQuery } from './normalize/query.js';

export { normalizeQuery } from './normalize/query.js';

/* ----------------------------- public API ------------------------------- */

export function normalizeInput(input: OqlSearchInput): OqlCanonicalInput {
  if (isBatchInput(input)) {
    return normalizeBatch(input);
  }
  return normalizeQuery(input as OqlInputQuery);
}
