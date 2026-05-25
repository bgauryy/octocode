/**
 * Shared TSV-envelope extension for tool output schemas.
 *
 * The bulk runner attaches `format: "tsv"` / `columns` / `rows` / `hints`
 * at the top level of every tool response. Upstream output schemas don't
 * know about these fields, so we wrap them here once and use the wrapper
 * at every tool registration point.
 */

import { z } from 'zod/v4';

export const tsvEnvelopeFields = {
  /** Output format marker — only present when format='tsv' was requested. */
  format: z.literal('tsv').optional(),
  /** TSV column header list (only when format='tsv'). */
  columns: z.array(z.string()).optional(),
  /** TSV row payload as a single tab-delimited string (only when format='tsv'). */
  rows: z.string().optional(),
  /** Top-level hints (response-state pagination / recovery / failure). */
  hints: z.array(z.string()).optional(),
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
