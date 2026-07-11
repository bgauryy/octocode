import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact } from './helpers.js';
import { fillScope } from './git.js';
import type { MemoryRow, MemoryRecord } from './types.js';
import { lexicalSearch } from './memory-search.js';

// ─── Decay / salience scoring ─────────────────────────────────────────────────

export const DECAY_WEIGHTS = { importance: 0.25, recency: 0.30, access: 0.15, lexical: 0.30 };
export const DEFAULT_HALF_LIFE_DAYS = 30.0;
export const ACCESS_SATURATION = 50.0;
// Weak-pool guard for lexical normalization: lexical = bm25 / (poolMax + K).
// Pure pool-max normalization gave the best hit of an all-weak pool relevance 1.0;
// K deflates weak pools while preserving within-pool ranking ratios. Calibrated
// against our column weights (10/7/2): good matches land at bm25 ≈ 1–3.
export const BM25_SQUASH_K = 1.0;
// Below this pool max, bm25 is degenerate (IDF collapse — e.g. a near-empty store
// or a term present in every row) and cannot discriminate; treat as neutral.
export const BM25_DEGENERATE_MAX = 0.01;
// Recall below this top-relevance is flagged judgment_required (engram BM25Floor pattern).
export const JUDGMENT_RELEVANCE_FLOOR = 0.35;
// Broad forget selectors (no explicit ids) never delete memories above this
// importance unless --max-importance explicitly raises the ceiling.
export const SALIENCE_FLOOR = 8;
// Per-label decay half-life defaults (days). Durable knowledge decays slowly;
// post-task reflections (EXPERIENCE) decay fast; everything else uses the
// read-time DEFAULT_HALF_LIFE_DAYS.
export const LABEL_HALF_LIFE_DAYS: Record<string, number> = {
  DECISION: 90, ARCHITECTURE: 90, SECURITY: 90, GOTCHA: 90,
  OVERRIDE: 90, // permanent corrections to model defaults — decay as slowly as DECISION
  EXPERIENCE: 14,
};
export const SCORING_PREFETCH_FACTOR = 3;
export const SIMILARITY_THRESHOLD = 0.45;
export const SIMILARITY_PREFETCH = 12;

export function canonicalMemoryInstant(value: string | null | undefined, field: string): string | null {
  if (value == null) return null;
  const text = String(value).trim();
  const isoInstant = /^\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2}))?$/;
  if (!isoInstant.test(text)) {
    throw new Error(`${field} must be a valid ISO 8601 timestamp`);
  }
  const parsed = new Date(text);
  if (!text || Number.isNaN(parsed.getTime())) {
    throw new Error(`${field} must be a valid ISO 8601 timestamp`);
  }
  return parsed.toISOString().replace(/\.\d{3}Z$/, 'Z');
}

/**
 * Tokenize text for Jaccard similarity. Splits camelCase/PascalCase,
 * lowercases, strips structural prefixes (file:, dir:, pr:, url:),
 * and filters short/stop tokens.
 */
export const STOP_WORDS = new Set([
  // Articles / conjunctions
  'the', 'and', 'for', 'with', 'from', 'into', 'not',
  // Demonstratives
  'this', 'that', 'its',
  // Question words
  'what', 'when', 'about', 'before', 'after',
  // Common verbs (too generic to be useful in memory search)
  'are', 'was', 'has', 'had', 'can', 'did', 'use', 'used', 'using',
]);

export function textTokens(text: string): Set<string> {
  // Split camelCase/PascalCase: workspacePath → workspace path
  const split = text
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2')
    .replace(/[:_-]/g, ' ')   // treat separators as spaces
    .toLowerCase();
  return new Set(
    (split.match(/[a-z0-9]{3,}/g) ?? [])
      .filter(t => !STOP_WORDS.has(t))
  );
}

export function jaccard(a: Set<string>, b: Set<string>): number {
  if (a.size === 0 || b.size === 0) return 0;
  let intersection = 0;
  for (const token of a) if (b.has(token)) intersection++;
  return intersection / (a.size + b.size - intersection);
}

export function findSimilarMemories(
  db: DatabaseSync,
  text: string,
  limit = 3,
  excludeMemoryId: string | null = null,
  scopeOptions: LexicalScopeOptions = {},
): Array<{ memory_id: string; similarity: number }> {
  const queryTokens = textTokens(text);
  if (queryTokens.size === 0) return [];

  const candidates = lexicalSearch(
    db, text, SIMILARITY_PREFETCH, 1, [], [], ['ACTIVE'], scopeOptions,
  ).filter(m => m.memory_id !== excludeMemoryId);

  return candidates
    .map(m => ({
      memory_id: m.memory_id,
      similarity: jaccard(queryTokens, textTokens(`${m.task_context} ${m.observation}`)),
    }))
    .filter(m => m.similarity >= SIMILARITY_THRESHOLD)
    .sort((a, b) => b.similarity - a.similarity)
    .slice(0, limit);
}

export function decayComponents(
  memory: MemoryRecord,
  lexical: number,
  weights = DECAY_WEIGHTS,
): NonNullable<MemoryRecord['score_components']> {
  const halfLife = memory.decay_half_life_days ?? DEFAULT_HALF_LIFE_DAYS;
  // Reading is popularity, not evidence freshness. Only source mutation time
  // may refresh recency; access_count remains a separate bounded component.
  const lastUsedStr = memory.updated_at ?? memory.created_at;
  let recency = 0;
  if (lastUsedStr) {
    const ageDays = Math.max(0, (Date.now() - new Date(lastUsedStr).getTime()) / 86400000);
    recency = Math.exp(-Math.LN2 * ageDays / Math.max(halfLife, 0.01));
  }
  const importance = (memory.importance ?? 0) / 10;
  const access = Math.min(
    Math.log1p(memory.access_count ?? 0) / Math.log1p(ACCESS_SATURATION), 1
  );
  const relevance = Math.max(0, Math.min(1, lexical));
  const final =
    weights.importance * importance +
    weights.recency * recency +
    weights.access * access +
    weights.lexical * relevance;
  return { importance, recency, access, relevance, weights, final };
}

export function decayScore(
  memory: MemoryRecord,
  lexical: number,
  weights = DECAY_WEIGHTS,
): number {
  return decayComponents(memory, lexical, weights).final;
}

// ─── FTS helpers ──────────────────────────────────────────────────────────────

export function buildFtsQuery(query: string): string | null {
  // Apply same camelCase split as textTokens so FTS matches what Jaccard compares.
  const normalized = query
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2')
    .replace(/[:_-]/g, ' ')
    .toLowerCase();
  const tokens = [
    ...new Set(
      (normalized.match(/[a-z0-9]{3,}/g) ?? []).filter(t => !STOP_WORDS.has(t))
    ),
  ].slice(0, 16);
  if (tokens.length === 0) return null;
  // Always OR: BM25 + decay scoring handles ranking; AND for short queries silently
  // dropped 2-token matches where only one term appears in a relevant memory.
  return tokens.join(' OR ');
}

export function escapeLike(value: string): string {
  return value.replace(/[\\%_]/g, '\\$&');
}

export function appendFallbackQueryConditions(
  query: string,
  conditions: string[],
  params: (string | number)[],
): void {
  const tokens = [...textTokens(query)].slice(0, 16);
  if (tokens.length === 0) return;

  const tokenClauses: string[] = [];
  for (const token of tokens) {
    const pattern = `%${escapeLike(token)}%`;
    tokenClauses.push(`(
      lower(m.task_context) LIKE ? ESCAPE '\\'
      OR lower(m.observation) LIKE ? ESCAPE '\\'
      OR lower(m.tags_json) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.label, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.workspace_path, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.artifact, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.repo, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.ref, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.failure_signature, '')) LIKE ? ESCAPE '\\'
      OR EXISTS (
        SELECT 1 FROM memory_refs r
        WHERE r.memory_id = m.memory_id
          AND lower(r.reference) LIKE ? ESCAPE '\\'
      )
    )`);
    params.push(pattern, pattern, pattern, pattern, pattern, pattern, pattern, pattern, pattern, pattern);
  }
  conditions.push(`(${tokenClauses.join(' OR ')})`);
}

export function fallbackSearch(
  db: DatabaseSync,
  query: string,
  conditions: string[],
  params: (string | number)[],
  limit: number,
): MemoryRow[] {
  const fallbackConditions = [...conditions];
  const fallbackParams = [...params];
  appendFallbackQueryConditions(query, fallbackConditions, fallbackParams);
  const sql = `
    SELECT m.*, 0 AS _bm25
    FROM memories m
    WHERE ${fallbackConditions.join(' AND ')}
    ORDER BY m.importance DESC, m.created_at DESC
    LIMIT ?
  `;
  return db.prepare(sql).all(...fallbackParams, limit) as unknown as MemoryRow[];
}

export interface LexicalScopeOptions {
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  strictScope?: boolean;
  globalOnly?: boolean;
  cwd?: string;
  asOf?: string | null;
  candidateMemoryIds?: string[];
}

export function applyScopeConditions(
  conditions: string[],
  params: (string | number)[],
  options: LexicalScopeOptions = {},
): void {
  const artifact = normalizeArtifact(options.artifact);
  const scope = fillScope(
    {
      workspace_path: options.workspacePath ?? null,
      artifact,
      repo: options.repo ?? null,
      ref: options.ref ?? null,
    },
    options.cwd ?? options.workspacePath ?? process.cwd(),
  );

  if (options.globalOnly) {
    conditions.push('m.workspace_path IS NULL', 'm.artifact IS NULL', 'm.repo IS NULL', 'm.ref IS NULL');
    return;
  }

  if (scope.workspace_path) {
    conditions.push(options.strictScope ? 'm.workspace_path = ?' : '(m.workspace_path IS NULL OR m.workspace_path = ?)');
    params.push(scope.workspace_path);
  }
  if (scope.artifact) {
    conditions.push(options.strictScope ? 'm.artifact = ?' : '(m.artifact IS NULL OR m.artifact = ?)');
    params.push(scope.artifact);
  }
  if (scope.repo) {
    conditions.push(options.strictScope ? 'm.repo = ?' : '(m.repo IS NULL OR m.repo = ?)');
    params.push(scope.repo);
  }
  if (scope.ref) {
    conditions.push(options.strictScope ? 'm.ref = ?' : '(m.ref IS NULL OR m.ref = ?)');
    params.push(scope.ref);
  }
}
