/**
 * Shared TSV-envelope extension for tool output schemas.
 *
 * The bulk runner attaches `format: "tsv"` / `columns` / `rows` / `hints`
 * at the top level of every tool response. Upstream output schemas don't
 * know about these fields, so we wrap them here once and use the wrapper
 * at every tool registration point.
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

export const tsvEnvelopeFields = {
  /** Output format marker — only present when format='tsv' was requested. */
  format: z.literal('tsv').optional(),
  /** TSV column header list (only when format='tsv'). */
  columns: z.array(z.string()).optional(),
  /** TSV row payload as a single tab-delimited string (only when format='tsv'). */
  rows: z.string().optional(),
  /** Top-level hints (response-state pagination / recovery / failure). */
  hints: z.array(z.string()).optional(),
  /** Cross-tool evidence metadata (kind / answerReady / confidence / complete). */
  evidence: EvidenceSchema,
} as const;

/**
 * Extend any object output schema with the TSV envelope fields above.
 *
 * The return type is `S` (the input schema), not the precise extended
 * shape. This is deliberate: callers don't access the envelope fields by
 * type — those are consumed by the bulk runner, not the agent — but they
 * DO access the data shape (`results`, `repositories`, `pull_requests`,
 * ...). Returning `S` lets test parses keep accessing the original
 * fields without `as` casts.
 *
 * At runtime the schema still validates the new fields as `.optional()`
 * additions, so MCP validation accepts the envelope and rejects garbage.
 */
export function withTsvEnvelope<S extends z.ZodObject>(schema: S): S {
  return schema.extend(tsvEnvelopeFields) as unknown as S;
}
