/**
 * memory.ts — Core memory store operations.
 *
 * insertMemory: pure DB insert, returns { memoryId, memory, superseded }.
 * getMemory:    FTS5 + decay-scored recall.
 * bumpAccess:   update access count and timestamp.
 */

import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';

import {
  normalizeArtifact, utcNow,
  normalizeTags, normalizeReferences, normalizeLabel, normalizeFilePath,
  rowToMemory, parseJsonList,
} from './helpers.js';
import { fillScope, normalizeWorkspacePath } from './git.js';
import { hasFts, ftsTermsForRow, replaceMemoryReferences } from './db.js';
import type {
  InsertMemoryParams, InsertMemoryResult, GetMemoryParams, GetMemoryResult,
  MemoryRow, MemoryRecord, ForgetMemoryParams, ForgetMemoryResult,
} from './types.js';

// ─── Decay / salience scoring ─────────────────────────────────────────────────

const DECAY_WEIGHTS = { importance: 0.25, recency: 0.30, access: 0.15, lexical: 0.30 };
const DEFAULT_HALF_LIFE_DAYS = 30.0;
const ACCESS_SATURATION = 50.0;
// Weak-pool guard for lexical normalization: lexical = bm25 / (poolMax + K).
// Pure pool-max normalization gave the best hit of an all-weak pool relevance 1.0;
// K deflates weak pools while preserving within-pool ranking ratios. Calibrated
// against our column weights (10/7/2): good matches land at bm25 ≈ 1–3.
const BM25_SQUASH_K = 1.0;
// Below this pool max, bm25 is degenerate (IDF collapse — e.g. a near-empty store
// or a term present in every row) and cannot discriminate; treat as neutral.
const BM25_DEGENERATE_MAX = 0.01;
// Recall below this top-relevance is flagged judgment_required (engram BM25Floor pattern).
const JUDGMENT_RELEVANCE_FLOOR = 0.35;
// Broad forget selectors (no explicit ids) never delete memories above this
// importance unless --max-importance explicitly raises the ceiling.
const SALIENCE_FLOOR = 8;
// Per-label decay half-life defaults (days). Durable knowledge decays slowly;
// post-task reflections (EXPERIENCE) decay fast; everything else uses the
// read-time DEFAULT_HALF_LIFE_DAYS.
const LABEL_HALF_LIFE_DAYS: Record<string, number> = {
  DECISION: 90, ARCHITECTURE: 90, SECURITY: 90, GOTCHA: 90,
  OVERRIDE: 90, // permanent corrections to model defaults — decay as slowly as DECISION
  EXPERIENCE: 14,
};
const SCORING_PREFETCH_FACTOR = 3;
const SIMILARITY_THRESHOLD = 0.45;
const SIMILARITY_PREFETCH = 12;

/**
 * Tokenize text for Jaccard similarity. Splits camelCase/PascalCase,
 * lowercases, strips structural prefixes (file:, dir:, pr:, url:),
 * and filters short/stop tokens.
 */
const STOP_WORDS = new Set([
  // Articles / conjunctions
  'the', 'and', 'for', 'with', 'from', 'into', 'not',
  // Demonstratives
  'this', 'that', 'its',
  // Question words
  'what', 'when', 'about', 'before', 'after',
  // Common verbs (too generic to be useful in memory search)
  'are', 'was', 'has', 'had', 'can', 'did', 'use', 'used', 'using',
]);

function textTokens(text: string): Set<string> {
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

function jaccard(a: Set<string>, b: Set<string>): number {
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
  const lastUsedStr = memory.last_accessed_at ?? memory.created_at;
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

function buildFtsQuery(query: string): string | null {
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

function escapeLike(value: string): string {
  return value.replace(/[\\%_]/g, '\\$&');
}

function appendFallbackQueryConditions(
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

function fallbackSearch(
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

interface LexicalScopeOptions {
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

function applyScopeConditions(
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

export function lexicalSearch(
  db: DatabaseSync,
  query: string,
  limit: number,
  minImportance: number,
  tags: string[],
  labels: string[],
  states: string[],
  scopeOptions: LexicalScopeOptions = {},
): MemoryRecord[] {
  const ftsQuery = query ? buildFtsQuery(query) : null;
  const params: (string | number)[] = [];
  const conditions: string[] = [
    'm.importance >= ?',
    `m.state IN (${states.map(() => '?').join(',')})`,
  ];
  params.push(minImportance, ...states);

  if (labels.length > 0) {
    conditions.push(`m.label IN (${labels.map(() => '?').join(',')})`);
    params.push(...labels);
  }
  // Use json_each subquery for tag filtering.
  for (const tag of tags) {
    conditions.push('EXISTS (SELECT 1 FROM json_each(m.tags_json) WHERE value = ?)');
    params.push(tag);
  }
  applyScopeConditions(conditions, params, scopeOptions);

  if (scopeOptions.asOf) {
    conditions.push('(m.valid_from IS NULL OR m.valid_from <= ?)');
    conditions.push('(m.valid_to IS NULL OR m.valid_to > ?)');
    params.push(scopeOptions.asOf, scopeOptions.asOf);
  }

  const candidateIds = scopeOptions.candidateMemoryIds
    ? [...new Set(scopeOptions.candidateMemoryIds)].filter(Boolean)
    : null;
  if (candidateIds && candidateIds.length === 0) return [];

  let usingCandidateTable = false;
  if (candidateIds) {
    if (candidateIds.length <= 400) {
      conditions.push(`m.memory_id IN (${candidateIds.map(() => '?').join(',')})`);
      params.push(...candidateIds);
    } else {
      db.exec('CREATE TEMP TABLE IF NOT EXISTS temp_memory_candidate_ids(memory_id TEXT PRIMARY KEY)');
      db.exec('DELETE FROM temp_memory_candidate_ids');
      const insertCandidate = db.prepare('INSERT OR IGNORE INTO temp_memory_candidate_ids(memory_id) VALUES (?)');
      for (const id of candidateIds) insertCandidate.run(id);
      conditions.push('EXISTS (SELECT 1 FROM temp_memory_candidate_ids c WHERE c.memory_id = m.memory_id)');
      usingCandidateTable = true;
    }
  }

  let rows: MemoryRow[];
  try {
    if (ftsQuery && hasFts(db)) {
      try {
        // BM25 column weights: memory_id=UNINDEXED(0), task_context=10, observation=7, tags=2.
        // Matches in task_context (what the agent was doing) score higher than
        // observation (the lesson) which scores higher than tags (supplementary).
        // bm25() returns negative values; ABS() + DESC gives best-match-first.
        const sql = `
          SELECT m.*, ABS(bm25(memories_fts, 0, 10, 7, 2)) AS _bm25
          FROM memories m
          JOIN memories_fts ON memories_fts.memory_id = m.memory_id
          WHERE memories_fts MATCH ?
            AND ${conditions.join(' AND ')}
          ORDER BY _bm25 DESC
          LIMIT ?
        `;
        rows = db.prepare(sql).all(ftsQuery, ...params, limit) as unknown as MemoryRow[];
      } catch {
        rows = fallbackSearch(db, query, conditions, params, limit);
      }
    } else {
      rows = fallbackSearch(db, query, conditions, params, limit);
    }
  } finally {
    if (usingCandidateTable) {
      try { db.exec('DELETE FROM temp_memory_candidate_ids'); } catch { /* non-critical cleanup */ }
    }
  }

  const maxBm25 = rows.reduce((m, r) => Math.max(m, r._bm25 ?? 0), 0);
  return rows.map(row => {
    const lexical = maxBm25 >= BM25_DEGENERATE_MAX
      ? (row._bm25 ?? 0) / (maxBm25 + BM25_SQUASH_K)
      : 0.5;
    const mem = rowToMemory(row);
    mem.lexical = lexical;
    mem.score = decayScore(mem, lexical);
    return mem;
  });
}

function attachMemoryReferences(db: DatabaseSync, memories: MemoryRecord[]): void {
  if (memories.length === 0) return;
  try {
    const ids = [...new Set(memories.map(m => m.memory_id))];
    const ph = ids.map(() => '?').join(',');
    const rows = db.prepare(
      `SELECT memory_id, reference
       FROM memory_refs
       WHERE memory_id IN (${ph})
       ORDER BY memory_id, ordinal`
    ).all(...ids) as unknown as Array<{ memory_id: string; reference: string }>;
    const refsByMemory = new Map<string, string[]>();
    for (const row of rows) {
      const refs = refsByMemory.get(row.memory_id) ?? [];
      refs.push(row.reference);
      refsByMemory.set(row.memory_id, refs);
    }
    for (const memory of memories) {
      memory.references = refsByMemory.get(memory.memory_id) ?? [];
    }
  } catch (e) {
    if (!(e instanceof Error && e.message.includes('no such table'))) throw e;
  }
}

function compileRecallRegex(pattern: string): RegExp {
  try {
    return new RegExp(pattern);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`invalid regex ${JSON.stringify(pattern)}: ${message}`);
  }
}

function intersectCandidateIds(current: Set<string> | null, next: Set<string>): Set<string> {
  if (current === null) return new Set(next);
  const out = new Set<string>();
  for (const id of current) if (next.has(id)) out.add(id);
  return out;
}

function exactReferenceCandidateIds(db: DatabaseSync, references: string[]): Set<string> {
  const refs = normalizeReferences(references);
  if (refs.length === 0) return new Set();
  const rows = db.prepare(
    `SELECT memory_id
     FROM memory_refs
     WHERE reference IN (${refs.map(() => '?').join(',')})
     GROUP BY memory_id
     HAVING COUNT(DISTINCT reference) = ?`
  ).all(...refs, refs.length) as unknown as Array<{ memory_id: string }>;
  return new Set(rows.map(row => row.memory_id));
}

function fileReferenceCandidates(files: string[], baseDir?: string | null): string[] {
  const refs = new Set<string>();
  for (const raw of files) {
    const file = String(raw ?? '').trim();
    if (!file) continue;
    refs.add(file);
    if (file.startsWith('file:')) {
      const unprefixed = file.slice(5);
      if (unprefixed) refs.add(unprefixed);
      continue;
    }
    refs.add(`file:${file}`);
    const normalized = normalizeFilePath(file, baseDir ?? undefined);
    if (normalized) {
      refs.add(normalized);
      refs.add(`file:${normalized}`);
    }
  }
  return [...refs];
}

function anyReferenceCandidateIds(db: DatabaseSync, references: string[]): Set<string> {
  const refs = [...new Set(references.map(ref => String(ref ?? '').trim().slice(0, 512)).filter(Boolean))];
  if (refs.length === 0) return new Set();
  const rows = db.prepare(
    `SELECT DISTINCT memory_id
     FROM memory_refs
     WHERE reference IN (${refs.map(() => '?').join(',')})`
  ).all(...refs) as unknown as Array<{ memory_id: string }>;
  return new Set(rows.map(row => row.memory_id));
}

function fileRegexCandidateIds(db: DatabaseSync, regexes: RegExp[]): Set<string> {
  if (regexes.length === 0) return new Set();
  const rows = db.prepare(
    `SELECT memory_id, reference
     FROM memory_refs
     WHERE kind = 'file' OR reference LIKE 'file:%'
     ORDER BY memory_id, ordinal`
  ).all() as unknown as Array<{ memory_id: string; reference: string }>;
  const refsByMemory = new Map<string, string[]>();
  for (const row of rows) {
    const refs = refsByMemory.get(row.memory_id) ?? [];
    refs.push(row.reference);
    refsByMemory.set(row.memory_id, refs);
  }
  const ids = new Set<string>();
  for (const [memoryId, refs] of refsByMemory.entries()) {
    if (regexes.every(re => refs.some(ref => re.test(ref)))) ids.add(memoryId);
  }
  return ids;
}

function regexCandidateIds(db: DatabaseSync, regexes: RegExp[]): Set<string> {
  if (regexes.length === 0) return new Set();
  type Row = MemoryRow & { references_text: string | null };
  const rows = db.prepare(
    `SELECT m.*, group_concat(r.reference, char(31)) AS references_text
     FROM memories m
     LEFT JOIN memory_refs r ON r.memory_id = m.memory_id
     GROUP BY m.memory_id`
  ).all() as unknown as Row[];
  const ids = new Set<string>();
  for (const row of rows) {
    const haystack = [
      row.task_context,
      row.observation,
      ...parseJsonList(row.tags_json),
      ...(row.references_text ? row.references_text.split('\u001f') : []),
      row.label,
      row.workspace_path,
      row.artifact,
      row.repo,
      row.ref,
      row.failure_signature,
    ].filter(Boolean).join(' ');
    if (regexes.every(re => re.test(haystack))) ids.add(row.memory_id);
  }
  return ids;
}

// ─── bumpAccess ───────────────────────────────────────────────────────────────

export function bumpAccess(db: DatabaseSync, memoryIds: string[]): void {
  if (memoryIds.length === 0) return;
  const now = utcNow();
  const placeholders = memoryIds.map(() => '?').join(',');
  db.prepare(`
    UPDATE memories
    SET access_count = COALESCE(access_count, 0) + 1, last_accessed_at = ?
    WHERE memory_id IN (${placeholders})
  `).run(now, ...memoryIds);
}

// ─── insertMemory ─────────────────────────────────────────────────────────────

/**
 * Insert a new memory record.
 * Returns { memoryId, memory, superseded } — does NOT emit JSON.
 */
export function insertMemory(db: DatabaseSync, params: InsertMemoryParams): InsertMemoryResult {
  const {
    agentId = 'agent',
    taskContext,
    observation,
    importance,
    label,
    tags = [],
    tagsCsv = '',
    references = [],
    supersedes = [],
    failureSignature = null,
    validFrom: vf,
    validTo: vt,
    workspacePath,
    artifact,
    repo: repoArg,
    ref: refArg,
    fileTreeFingerprint = null,
    cwd,
  } = params;

  const imp = Number(importance);
  if (!Number.isInteger(imp) || imp < 1 || imp > 10) {
    throw new Error(`importance must be 1–10, got ${String(importance)}`);
  }

  const memoryId = 'mem_' + randomUUID().replace(/-/g, '');
  const tagList = normalizeTags(tags, tagsCsv);
  const refList = normalizeReferences(references);
  const normalizedLabel = normalizeLabel(Array.isArray(label) ? label[0] : label);
  const createdAt = utcNow();
  const validFromVal = vf ?? createdAt;

  const scope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );

  const halfLifeDefault = LABEL_HALF_LIFE_DAYS[normalizedLabel] ?? null;

  // Variables assigned inside the transaction (declared outside for return scope).
  let noveltyScore = 0;
  let similarMemoryIds: string[] = [];
  const superseded: string[] = [];

  db.exec('BEGIN IMMEDIATE');
  try {
    // FIX #8 (P1): findSimilarMemories moved inside the transaction for read consistency —
    // ensures the similarity check and the insert see the same set of ACTIVE memories.
    // TOOL-2: Use preComputedSimilar if provided (avoids double findSimilarMemories call
    // when the caller already ran a dedup gate check before deciding to insert).
    const similar = params.preComputedSimilar ?? findSimilarMemories(db, `${taskContext} ${observation}`, 3, null, {
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      cwd,
    });
    noveltyScore = Math.max(0, Math.min(1, 1 - (similar[0]?.similarity ?? 0)));
    similarMemoryIds = similar.map(m => m.memory_id);

    db.prepare(`
      INSERT INTO memories (
        memory_id, agent_id, task_context, observation, importance,
        label, tags_json, workspace_path, artifact, repo, ref,
        file_tree_fingerprint, novelty_score, created_at, updated_at,
        last_accessed_at, access_count, failure_signature, valid_from, valid_to, decay_half_life_days
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?)
    `).run(
      memoryId, agentId, taskContext, observation, imp,
      normalizedLabel, JSON.stringify(tagList),
      scope.workspace_path, scope.artifact, scope.repo, scope.ref,
      fileTreeFingerprint, noveltyScore, createdAt, createdAt,
      createdAt, failureSignature ?? null, validFromVal, vt ?? null, halfLifeDefault
    );

    // Populate structured reference index (memory_refs table)
    if (refList.length > 0) {
      try {
        replaceMemoryReferences(db, memoryId, refList);
      } catch (e) {
        if (!(e instanceof Error && e.message.includes('no such table'))) throw e;
      }
    }

    if (hasFts(db)) {
      db.prepare(
        'INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)'
      ).run(
        memoryId, taskContext, observation,
        ftsTermsForRow({
          tags_json: JSON.stringify(tagList),
          label: normalizedLabel,
          references: refList,
        })
      );
    }
    // FIX #1 (P0): supersede UPDATE loop moved INSIDE BEGIN IMMEDIATE (before COMMIT)
    // so the supersede and insert are atomic — no window where the old memory is still
    // ACTIVE after the new one is visible to concurrent readers.
    for (const oldId of supersedes) {
      const r = db.prepare(`
        UPDATE memories
        SET state = 'SUPERSEDED', superseded_by = ?, updated_at = ?,
            valid_to = COALESCE(valid_to, ?), expired_at = ?
        WHERE memory_id = ? AND memory_id <> ?
      `).run(memoryId, createdAt, validFromVal, createdAt, oldId, memoryId) as { changes: number };
      if (r.changes) superseded.push(oldId);
    }

    db.exec('COMMIT');
  } catch (e) {
    try { db.exec('ROLLBACK'); } catch { /* already rolled back */ }
    throw e;
  }

  return {
    memoryId,
    memory: {
      memory_id: memoryId,
      agent_id: agentId,
      task_context: taskContext,
      observation,
      importance: imp,
      label: normalizedLabel,
      tags: tagList,
      references: refList,
      workspace_path: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      failure_signature: failureSignature ?? null,
      novelty_score: noveltyScore,
      state: 'ACTIVE' as const,
      created_at: createdAt,
    },
    superseded,
    noveltyScore,
    similarMemoryIds,
  };
}

// ─── getMemory ────────────────────────────────────────────────────────────────

/**
 * Recall memories using FTS5 + decay scoring.
 */
export function getMemory(db: DatabaseSync, params: GetMemoryParams = {}): GetMemoryResult {
  const {
    query = '',
    limit: limitRaw = 3,
    minImportance: minImpRaw = 1,
    label,
    tags = [],
    smart = false,
    workspacePath,
    artifact,
    repo: repoArg,
    ref: refArg,
    states: statesRaw,
    sort = 'smart',
    globalOnly = false,
    strictScope = false,
    asOf,
    references = [],
    regex = [],
    fileRegex = [],
    files = [],
    explain = false,
    cwd: cwdParam,
  } = params;

  const limit = Math.min(20, Math.max(1, Number(limitRaw) || 3));
  let minImportance = Math.max(1, Number(minImpRaw) || 1);
  if (smart === true || smart === 'true') minImportance = Math.max(1, minImportance - 1);

  const states = statesRaw ?? ['ACTIVE'];
  const labels = label
    ? (Array.isArray(label) ? label.map(normalizeLabel) : [normalizeLabel(label)])
    : [];

  const effectiveCwd = cwdParam ?? workspacePath ?? undefined;
  const asOfDate = asOf ? new Date(asOf) : null;
  if (asOfDate && isNaN(asOfDate.getTime())) {
    throw new Error(`invalid --as-of value "${asOf}" — expected ISO 8601 date string (e.g. 2024-06-01T00:00:00Z)`);
  }

  let candidateIds: Set<string> | null = null;
  const refFilters = normalizeReferences(references);
  const fileRefFilters = fileReferenceCandidates(files, effectiveCwd);
  const compiledRegex = regex.map(compileRecallRegex);
  const compiledFileRegex = fileRegex.map(compileRecallRegex);

  if (refFilters.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, exactReferenceCandidateIds(db, refFilters));
  }
  if (fileRefFilters.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, anyReferenceCandidateIds(db, fileRefFilters));
  }
  if (compiledFileRegex.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, fileRegexCandidateIds(db, compiledFileRegex));
  }
  if (compiledRegex.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, regexCandidateIds(db, compiledRegex));
  }

  let memories = lexicalSearch(
    db, query, limit * SCORING_PREFETCH_FACTOR, minImportance, tags, labels, states, {
      workspacePath: workspacePath ?? cwdParam ?? null,
      artifact,
      repo: repoArg,
      ref: refArg,
      strictScope,
      globalOnly,
      cwd: cwdParam,
      asOf: asOf ?? null,
      candidateMemoryIds: candidateIds ? [...candidateIds] : undefined,
    },
  );
  attachMemoryReferences(db, memories);

  // Exact file filter — the `file` column was removed from the schema (files are
  // now tracked via memory_refs with prefix "file:"). For forward compatibility we
  // keep the filter logic but match against the references array instead.
  if (fileRefFilters.length > 0) {
    const normFiles = new Set(fileRefFilters);
    memories = memories.filter(m =>
      m.references.some(r => normFiles.has(r))
    );
  }

  // Reference filters are conjunctive: every provided provenance reference must
  // be present. This is pushed down before the search cap and rechecked here.
  if (refFilters.length > 0) {
    memories = memories.filter(m => refFilters.every(ref => m.references.includes(ref)));
  }

  // Regex filter
  if (compiledRegex.length > 0 || compiledFileRegex.length > 0) {
    memories = memories.filter(m => {
      if (compiledFileRegex.length > 0) {
        // Match fileRegex against file: prefixed references (no standalone file column)
        const fileRefs = (m.references ?? []).filter(r => r.startsWith('file:'));
        if (!compiledFileRegex.every(re => fileRefs.some(r => re.test(r)))) return false;
      }
      if (compiledRegex.length > 0) {
        const haystack = [
          m.task_context, m.observation,
          ...(m.tags ?? []), ...(m.references ?? []),
          m.label, m.workspace_path, m.artifact, m.repo, m.ref, m.failure_signature,
        ].filter(Boolean).join(' ');
        if (!compiledRegex.every(re => re.test(haystack))) return false;
      }
      return true;
    });
  }

  if (asOfDate) {
    memories = memories.filter(m => {
      const vf = m.valid_from ? new Date(m.valid_from) : null;
      const vt = m.valid_to ? new Date(m.valid_to) : null;
      return (!vf || vf <= asOfDate) && (!vt || vt > asOfDate);
    });
  }

  // Sort
  if (sort === 'importance') {
    memories.sort((a, b) =>
      (b.importance - a.importance) || ((b.score ?? 0) - (a.score ?? 0)));
  } else if (sort === 'recent') {
    memories.sort((a, b) => (b.created_at ?? '').localeCompare(a.created_at ?? ''));
  } else if (sort === 'accessed') {
    memories.sort((a, b) =>
      (b.last_accessed_at ?? b.created_at ?? '').localeCompare(a.last_accessed_at ?? a.created_at ?? ''));
  } else {
    memories.sort((a, b) => (b.score ?? 0) - (a.score ?? 0));
  }

  memories = memories.slice(0, limit);
  if (explain) {
    for (const m of memories) {
      const components = decayComponents(m, m.lexical ?? 0);
      m.score_components = components;
      m.score = components.final;
    }
  }
  bumpAccess(db, memories.map(m => m.memory_id));

  const mode = hasFts(db) ? 'lexical' as const : 'fallback' as const;
  const result: GetMemoryResult = {
    count: memories.length,
    memories,
    mode,
    sort,
    as_of: asOf ?? null,
    global_only: Boolean(globalOnly),
    states,
  };

  // Low-confidence recall flag (engram BM25Floor pattern): tell the agent when
  // results are weak matches so it verifies before relying on them.
  if (query.trim()) {
    const topRelevance = memories[0]?.lexical ?? 0;
    if (memories.length === 0) {
      result.judgment_required = true;
      result.judgment_reason = 'no results — absence of recall is not proof of absence; retry with --smart or broader terms';
    } else if (mode === 'fallback') {
      result.judgment_required = true;
      result.judgment_reason = 'FTS unavailable — results are unranked substring matches; verify relevance before relying on them';
    } else if (topRelevance < JUDGMENT_RELEVANCE_FLOOR) {
      result.judgment_required = true;
      result.judgment_reason = `weak lexical match (top relevance ${topRelevance.toFixed(2)} < ${JUDGMENT_RELEVANCE_FLOOR}) — treat results as leads, not answers`;
    }
  }

  return result;
}

// ─── forgetMemory ─────────────────────────────────────────────────────────────────

/**
 * Delete memories by id, tag, age, or importance ceiling.
 * dryRun=true returns the count without deleting anything.
 */
export function forgetMemory(db: DatabaseSync, params: ForgetMemoryParams): ForgetMemoryResult {
  const { memoryIds = [], tags = [], before, dryRun = false } = params;
  let { maxImportance } = params;
  const scope = fillScope(
    {
      workspace_path: params.workspacePath ?? null,
      artifact: normalizeArtifact(params.artifact),
      repo: params.repo ?? null,
      ref: params.ref ?? null,
    },
    params.cwd ?? params.workspacePath ?? process.cwd(),
  );

  // Two independent OR-combined selector groups so filters don't cross-contaminate:
  //   Group 1 — explicit IDs: selected directly, no importance or tag filter applied.
  //             (combining id + maxImportance as AND silently deleted nothing when the
  //             target memory had higher importance than the ceiling — docstring says OR.)
  //   Group 2 — attribute-based: tags/age/importance with salience-floor guard.
  // Optional scope flags are AND-combined with either selector group.
  const selectorGroups: string[] = [];
  const bindParams: (string | number)[] = [];
  let salienceFloorApplied = false;

  // Group 1: direct by id (unconditional)
  if (memoryIds.length > 0) {
    selectorGroups.push(`memory_id IN (${memoryIds.map(() => '?').join(',')})`);
    bindParams.push(...memoryIds);
  }

  // Group 2: attribute-based (tags + age + importance ceiling)
  const attrConds: string[] = [];
  const attrBinds: (string | number)[] = [];
  if (tags.length > 0) {
    // Use json_each subquery for tag filtering.
    attrConds.push(
      `(${tags.map(() => 'EXISTS (SELECT 1 FROM json_each(tags_json) WHERE value = ?)').join(' OR ')})`
    );
    attrBinds.push(...tags);
  }
  if (before) {
    attrConds.push('created_at < ?');
    attrBinds.push(before);
  }
  if (attrConds.length > 0 || maxImportance != null) {
    // Salience floor: broad attribute selectors never sweep high-importance memories
    // unless --max-importance explicitly raises the ceiling.
    if (maxImportance == null) {
      maxImportance = SALIENCE_FLOOR - 1;
      salienceFloorApplied = true;
    }
    attrConds.push('importance <= ?');
    attrBinds.push(maxImportance);
    selectorGroups.push(`(${attrConds.join(' AND ')})`);
    bindParams.push(...attrBinds);
  }

  if (selectorGroups.length === 0) {
    throw new Error('forgetMemory requires at least one filter: memoryIds, tags, before, or maxImportance');
  }

  const scopeConds: string[] = [];
  const scopeBinds: (string | number)[] = [];
  if (params.workspacePath && scope.workspace_path) {
    scopeConds.push('workspace_path = ?');
    scopeBinds.push(scope.workspace_path);
  }
  if (params.artifact && scope.artifact) {
    scopeConds.push('artifact = ?');
    scopeBinds.push(scope.artifact);
  }
  if (params.repo && scope.repo) {
    scopeConds.push('repo = ?');
    scopeBinds.push(scope.repo);
  }
  if (params.ref && scope.ref) {
    scopeConds.push('ref = ?');
    scopeBinds.push(scope.ref);
  }

  const selectorWhere = selectorGroups.join(' OR ');
  const where = scopeConds.length > 0
    ? `(${selectorWhere}) AND ${scopeConds.join(' AND ')}`
    : selectorWhere;
  const rows = db.prepare(
    `SELECT memory_id FROM memories WHERE ${where}`
  ).all(...bindParams, ...scopeBinds) as unknown as Array<{ memory_id: string }>;
  const ids = rows.map(r => r.memory_id);

  if (dryRun) {
    return {
      deleted: 0, dry_run: true, would_delete: ids.length, memory_ids: ids,
      ...(salienceFloorApplied ? { salience_floor: SALIENCE_FLOOR } : {}),
    };
  }

  // FIX #5 (P0): wrap all three DELETEs in a transaction so FTS and refs rows are
  // never left orphaned if one of the deletes fails mid-way.
  if (ids.length > 0) {
    const ph = ids.map(() => '?').join(',');
    db.exec('BEGIN IMMEDIATE');
    try {
      db.prepare(`DELETE FROM memories WHERE memory_id IN (${ph})`).run(...ids);
      if (hasFts(db)) {
        db.prepare(`DELETE FROM memories_fts WHERE memory_id IN (${ph})`).run(...ids);
      }
      try {
        db.prepare(`DELETE FROM memory_refs WHERE memory_id IN (${ph})`).run(...ids);
      } catch { /* ignore if table missing */ }
      db.exec('COMMIT');
    } catch (e) {
      try { db.exec('ROLLBACK'); } catch { /* already rolled back */ }
      throw e;
    }
  }

  return {
    deleted: ids.length, memory_ids: ids,
    ...(salienceFloorApplied ? { salience_floor: SALIENCE_FLOOR } : {}),
  };
}

// ─── mineWeakness ─────────────────────────────────────────────────────────────

export interface WeaknessCluster {
  failure_signature: string;  // raw (may include |surface:Z suffix)
  base_signature: string;     // without |surface:Z — use this for display/grouping
  surfaces: string[];         // extracted surface values across all merged signatures
  count: number;
  avg_importance: number;
  score: number;
  memory_ids: string[];
  representative: string;
  labels: string[];
}

export interface MineWeaknessResult {
  ok: true;
  clusters: WeaknessCluster[];
  total_signatures: number;
  total_memories: number;
}

export interface MineWeaknessParams {
  agentId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  minCount?: number;
  limit?: number;
  cwd?: string;
}

/** Strip optional |surface:Z suffix from a failure_signature for cluster merging. */
function stripSurface(sig: string): string {
  const idx = sig.indexOf('|surface:');
  return idx >= 0 ? sig.slice(0, idx) : sig;
}

/** Extract the |surface:Z value if present. */
function extractSurface(sig: string): string | null {
  const idx = sig.indexOf('|surface:');
  return idx >= 0 ? sig.slice(idx + 9) : null;
}

/** Tokenize a failure_signature for Jaccard similarity (splits on |:). */
function sigTokens(sig: string): Set<string> {
  return new Set(
    sig.split(/[|:]+/)
      .map(s => s.trim().toLowerCase())
      .filter(s => s.length > 2 && s !== 'mechanism' && s !== 'cause' && s !== 'surface'),
  );
}

/**
 * Cluster memories by failure_signature to surface recurring failure patterns.
 * Sorted by count × avg_importance so the most impactful patterns appear first.
 *
 * Improvements vs naive GROUP BY:
 * 1. Signatures differing only in |surface:Z suffix are merged into one cluster
 *    (base_signature is the cluster key; surfaces[] collects all variants).
 * 2. Diversity filter: a cluster is suppressed if Jaccard similarity ≥ 0.5 vs
 *    any already-selected cluster, so the output covers distinct failure mechanisms
 *    rather than N variants of the same one.
 */
export function mineWeakness(db: DatabaseSync, params: MineWeaknessParams = {}): MineWeaknessResult {
  const { minCount = 2, limit = 20, cwd } = params;
  const wsPath = params.workspacePath
    ? normalizeWorkspacePath(params.workspacePath, params.workspacePath)
    : (cwd ? normalizeWorkspacePath(null, cwd) : null);
  const artifact = normalizeArtifact(params.artifact);

  const conditions: string[] = ["failure_signature IS NOT NULL", "state = 'ACTIVE'"];
  const bindParams: (string | number)[] = [];
  if (wsPath) { conditions.push('(workspace_path = ? OR workspace_path IS NULL)'); bindParams.push(wsPath); }
  if (artifact) { conditions.push('(artifact = ? OR artifact IS NULL)'); bindParams.push(artifact); }
  if (params.agentId) { conditions.push('agent_id = ?'); bindParams.push(params.agentId); }

  type ClusterRow = { failure_signature: string; freq: number; avg_imp: number; score: number; ids: string; labels: string };
  const rows = db.prepare(`
    SELECT failure_signature,
           count(*) AS freq,
           avg(importance) AS avg_imp,
           count(*) * avg(importance) AS score,
           group_concat(memory_id, ',') AS ids,
           group_concat(DISTINCT label) AS labels
    FROM memories
    WHERE ${conditions.join(' AND ')}
    GROUP BY failure_signature
    ORDER BY score DESC
  `).all(...bindParams) as unknown as ClusterRow[];

  // Phase 1: merge rows that share the same base_signature (differ only in |surface:Z).
  // Key = stripped signature; value = merged cluster accumulators.
  interface Merged {
    base_sig: string;
    raw_sig: string;    // highest-score raw signature (for rep lookup)
    total_freq: number;
    total_score: number;
    importance_sum: number;
    ids: string[];
    labels: Set<string>;
    surfaces: Set<string>;
    raw_score: number;
  }
  const mergedMap = new Map<string, Merged>();
  for (const row of rows) {
    const base = stripSurface(row.failure_signature);
    const surface = extractSurface(row.failure_signature);
    const existing = mergedMap.get(base);
    if (existing) {
      existing.total_freq += row.freq;
      existing.total_score += row.score;
      existing.importance_sum += row.avg_imp * row.freq;
      existing.ids.push(...row.ids.split(','));
      for (const l of row.labels.split(',').filter(Boolean)) existing.labels.add(l);
      if (surface) existing.surfaces.add(surface);
      // Keep the raw signature with the highest original score for rep lookup
      if (row.score > existing.raw_score) {
        existing.raw_sig = row.failure_signature;
        existing.raw_score = row.score;
      }
    } else {
      mergedMap.set(base, {
        base_sig: base,
        raw_sig: row.failure_signature,
        total_freq: row.freq,
        total_score: row.score,
        importance_sum: row.avg_imp * row.freq,
        ids: row.ids.split(','),
        labels: new Set(row.labels.split(',').filter(Boolean)),
        surfaces: new Set(surface ? [surface] : []),
        raw_score: row.score,
      });
    }
  }

  // Re-sort merged clusters by total_score DESC.
  const merged = [...mergedMap.values()]
    .filter(m => m.total_freq >= minCount)
    .sort((a, b) => b.total_score - a.total_score);

  // Phase 2: batch-fetch representatives for all distinct base signatures.
  type RepRow = { failure_signature: string; observation: string };
  const repMap = new Map<string, string>();
  const allRawSigs = merged.map(m => m.raw_sig);
  if (allRawSigs.length > 0) {
    const ph = allRawSigs.map(() => '?').join(',');
    const repRows = db.prepare(
      `SELECT failure_signature, observation, max(importance)
       FROM memories
       WHERE failure_signature IN (${ph}) AND ${conditions.join(' AND ')}
       GROUP BY failure_signature`
    ).all(...allRawSigs, ...bindParams) as unknown as RepRow[];
    for (const r of repRows) repMap.set(stripSurface(r.failure_signature), r.observation);
  }

  // Phase 3: Jaccard diversity filter — skip cluster if ≥ 0.5 overlap with any already-selected.
  const selected: WeaknessCluster[] = [];
  for (const m of merged) {
    if (selected.length >= limit) break;
    const toks = sigTokens(m.base_sig);
    // FIX #10: use single jaccard() function (jaccardSimilarity removed as redundant duplicate).
    const tooSimilar = selected.some(
      sel => jaccard(sigTokens(sel.base_signature), toks) >= 0.5,
    );
    if (tooSimilar) continue;
    selected.push({
      failure_signature: m.raw_sig,
      base_signature: m.base_sig,
      surfaces: [...m.surfaces].sort(),
      count: m.total_freq,
      avg_importance: Math.round((m.importance_sum / m.total_freq) * 10) / 10,
      score: Math.round(m.total_score * 10) / 10,
      memory_ids: [...new Set(m.ids)],
      representative: (repMap.get(m.base_sig) ?? '').slice(0, 200),
      labels: [...m.labels].sort(),
    });
  }

  type TotalRow = { sigs: number; mems: number };
  const totals = db.prepare(
    `SELECT count(DISTINCT failure_signature) AS sigs, count(*) AS mems
     FROM memories WHERE ${conditions.join(' AND ')}`
  ).get(...bindParams) as unknown as TotalRow;

  return { ok: true, clusters: selected, total_signatures: totals.sigs, total_memories: totals.mems };
}

// ─── Embedding storage + cosine search (ARCH-6) ─────────────────────────────

/**
 * Compute cosine similarity between two Float32 vectors.
 * Returns 0 if either vector has zero magnitude.
 */
function cosineSimilarity(a: Float32Array, b: Float32Array): number {
  if (a.length !== b.length || a.length === 0) return 0;
  let dot = 0, normA = 0, normB = 0;
  for (let i = 0; i < a.length; i++) {
    dot  += a[i]! * b[i]!;
    normA += a[i]! * a[i]!;
    normB += b[i]! * b[i]!;
  }
  const denom = Math.sqrt(normA) * Math.sqrt(normB);
  return denom === 0 ? 0 : dot / denom;
}

/**
 * Store a dense embedding for a memory.
 * Uses the existing `embedding` BLOB + `embedding_model` TEXT columns
 * (already in the schema; previously unused).
 *
 * The embedding source (API, local model) is the caller's responsibility —
 * this function only handles persistence.
 *
 * @param embedding - Flat Float32Array from a text-embedding model
 * @param model     - Model identifier, e.g. 'text-embedding-3-small'
 */
export function storeEmbedding(
  db: DatabaseSync,
  memoryId: string,
  embedding: Float32Array,
  model: string,
): void {
  // Serialize Float32Array → raw binary buffer stored as BLOB
  const blob = Buffer.from(embedding.buffer, embedding.byteOffset, embedding.byteLength);
  db.prepare(
    `UPDATE memories SET embedding = ?, embedding_model = ?, updated_at = ?
     WHERE memory_id = ?`
  ).run(blob, model, utcNow(), memoryId);
}

/**
 * Search memories by cosine similarity against a query embedding.
 *
 * Retrieves all stored embeddings (optionally filtered by model) and ranks
 * them in JS. For stores < ~10k memories this is fast enough; at larger
 * scale, a proper vector index (e.g. sqlite-vss) would be needed.
 *
 * @param queryEmbedding - The embedding of the text to search for
 * @param limit          - Maximum results to return (default 5)
 * @param threshold      - Minimum cosine similarity 0–1 (default 0.75)
 * @param model          - Only compare against embeddings from this model
 */
export function searchByEmbedding(
  db: DatabaseSync,
  queryEmbedding: Float32Array,
  limit = 5,
  threshold = 0.75,
  model?: string,
): Array<{ memory_id: string; similarity: number }> {
  const conditions = ["state = 'ACTIVE'", 'embedding IS NOT NULL'];
  const binds: string[] = [];
  if (model) { conditions.push('embedding_model = ?'); binds.push(model); }

  type EmbRow = { memory_id: string; embedding: Buffer; embedding_model: string };
  // Limit to avoid loading unbounded embedding blobs into JS heap; cosine-rank within the cap.
  const rows = db.prepare(
    `SELECT memory_id, embedding, embedding_model FROM memories
     WHERE ${conditions.join(' AND ')}
     ORDER BY COALESCE(last_accessed_at, created_at) DESC
     LIMIT 2000`
  ).all(...binds) as unknown as EmbRow[];

  const results: Array<{ memory_id: string; similarity: number }> = [];
  for (const row of rows) {
    try {
      const stored = new Float32Array(
        row.embedding.buffer,
        row.embedding.byteOffset,
        row.embedding.byteLength / 4,
      );
      const sim = cosineSimilarity(queryEmbedding, stored);
      if (sim >= threshold) results.push({ memory_id: row.memory_id, similarity: sim });
    } catch { /* corrupted BLOB — skip */ }
  }

  return results
    .sort((a, b) => b.similarity - a.similarity)
    .slice(0, limit);
}

/**
 * Load ACTIVE memory rows by id, preserving the caller order when possible.
 */
export function loadMemoriesByIds(
  db: DatabaseSync,
  memoryIds: string[],
): MemoryRecord[] {
  const ids = [...new Set(memoryIds.filter(Boolean))];
  if (ids.length === 0) return [];
  const placeholders = ids.map(() => '?').join(', ');
  const rows = db.prepare(
    `SELECT * FROM memories WHERE memory_id IN (${placeholders}) AND state = 'ACTIVE'`
  ).all(...ids) as unknown as MemoryRow[];
  const byId = new Map(rows.map(row => [row.memory_id, rowToMemory(row)]));
  attachMemoryReferences(db, [...byId.values()]);
  return ids.map(id => byId.get(id)).filter((row): row is MemoryRecord => Boolean(row));
}
