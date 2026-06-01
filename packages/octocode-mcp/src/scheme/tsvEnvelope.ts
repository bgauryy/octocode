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
  /**
   * Common directory the `path` cells are relative to (lean-output hoisting).
   * Absolute path = `${base}/${row.path}`. structuredContent keeps absolute
   * paths, so this only affects the compact TSV the agent reads.
   */
  base: z.string().optional(),
  /**
   * Columns lifted out of every row because they shared one identical value
   * (e.g. owner/repo of a single-repo search). Emitted once instead of per row.
   */
  shared: z.record(z.string(), z.string()).optional(),
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

/**
 * Presentation-only TSV envelope keys. These hold a flattened, stringified copy
 * of data that already lives in the structured records (`results` /
 * `repositories` / `pull_requests` / ...). They are a token optimization for the
 * model-facing `content[0].text` only — keeping them in `structuredContent`
 * too would serialize the same rows twice. `hints` / `evidence` are NOT copies
 * of the data and intentionally stay in both. (#A1)
 */
// Pure-presentation keys stripped from structuredContent (they only shape the
// content[0].text TSV). `base` is intentionally NOT here: once paths in the
// canonical records are relativized, `base` is data-bearing — the model needs
// it to reconstruct `abs = ${base}/${path}` — so it must survive into
// structuredContent alongside the relativized paths.
const TSV_PRESENTATION_KEYS = ['format', 'columns', 'rows', 'shared'] as const;

/**
 * Return a shallow copy of a bulk response with the presentation-only TSV
 * envelope removed, for use as `structuredContent`. Returns the input unchanged
 * (no copy) when none of the keys are present (e.g. JSON mode), so it is cheap
 * on the common path.
 */
export function stripTsvEnvelope<T extends object>(data: T): T {
  const record = data as Record<string, unknown>;
  let copy: Record<string, unknown> | undefined;
  for (const key of TSV_PRESENTATION_KEYS) {
    if (key in record) {
      copy ??= { ...record };
      delete copy[key];
    }
  }
  return (copy ?? data) as T;
}
