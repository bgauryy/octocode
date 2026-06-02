/**
 * Shared response-envelope extension for tool output schemas.
 *
 * The bulk runner attaches peer-level `hints`, a `base` path prefix, and
 * cross-tool `evidence` metadata at the top level of every tool response.
 * Upstream output schemas don't know about these fields, so we wrap them here
 * once and use the wrapper at every tool registration point.
 */

import { z } from 'zod/v4';

/**
 * Shared evidence metadata. Tools opt in to populating these fields so the
 * agent can tell whether a response is answer-ready, complete, and what
 * kind of evidence was returned — without parsing the payload shape.
 */
export const EvidenceSchema = z
  .object({
    /** What category of evidence this response carries. */
    kind: z
      .enum([
        'metadata',
        'content',
        'structure',
        'code',
        'docs',
        'config',
        'pr',
        'repo',
        'package',
        'definition',
        'references',
        'calls',
      ])
      .optional(),
    /** True when the response contains enough to answer the caller's intent. */
    answerReady: z.boolean().optional(),
    /** How much to trust this evidence (semantic vs. heuristic vs. fallback). */
    confidence: z.enum(['high', 'medium', 'low']).optional(),
    /** False if results were truncated / paginated and more remain. */
    complete: z.boolean().optional(),
    /** Short human-readable reason explaining the state above. */
    reason: z.string().optional(),
    /** Names of fields the caller asked for but the tool could not return. */
    missingFields: z.array(z.string()).optional(),
  })
  .optional();

export const responseEnvelopeFields = {
  /** Top-level hints (response-state pagination / recovery / failure). */
  hints: z.array(z.string()).optional(),
  /**
   * Common directory the `path`/`uri` cells are relative to (lean-output
   * hoisting). Absolute path = `${base}/${path}`. The canonical paths in the
   * response are relativized against this, so the model reconstructs the
   * absolute path from it.
   */
  base: z.string().optional(),
  /**
   * Scalar fields hoisted out of every leaf object because they shared one
   * identical value (e.g. `type`/`permissions` across all files). Emitted once
   * instead of per leaf; each leaf re-gains every key on reconstruction.
   */
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
  /** Cross-tool evidence metadata (kind / answerReady / confidence / complete). */
  evidence: EvidenceSchema,
} as const;

/**
 * Extend any object output schema with the response envelope fields above.
 *
 * Return type is pinned to the INPUT schema `S`, not the precise extended
 * shape. This is a deliberate, load-bearing compile-time bridge — NOT a
 * cosmetic gap:
 *
 *  - The envelope fields (`hints`/`base`/`evidence`) are attached and validated
 *    at RUNTIME, consumed by the bulk runner — never read off the type by
 *    callers.
 *  - Pinning to `S` keeps `withResponseEnvelope(X)` assignable EVERYWHERE `X`
 *    is — most importantly the `BulkOutputSchema`-typed parameters that
 *    consumers (registration `outputSchema`, test harnesses like
 *    `expectHasResultsData`) require. Returning the precise `schema.extend(...)`
 *    type instead breaks that assignability (the extended ZodObject is a
 *    different, wider generic), which a type-check confirms surfaces at those
 *    call sites.
 *
 * `as unknown as S` (double cast) is the minimal way to express this: a single
 * `as S` is rejected because the extended type doesn't sufficiently overlap `S`.
 */
export function withResponseEnvelope<S extends z.ZodObject>(schema: S): S {
  return schema.extend(responseEnvelopeFields) as unknown as S;
}
