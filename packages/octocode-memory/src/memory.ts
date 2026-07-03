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
  utcNow, tagsText,
  normalizeTags, normalizeReferences, normalizeLabel, normalizeFilePath,
  rowToMemory,
} from './helpers.js';
import { fillScope } from './git.js';
import { hasFts, ftsTermsForRow } from './db.js';
import type {
  InsertMemoryParams, InsertMemoryResult, GetMemoryParams, GetMemoryResult,
  MemoryRow, MemoryRecord,
} from './types.js';

// ─── Decay / salience scoring ─────────────────────────────────────────────────

const DECAY_WEIGHTS = { importance: 0.25, recency: 0.30, access: 0.15, lexical: 0.30 };
const DEFAULT_HALF_LIFE_DAYS = 30.0;
const ACCESS_SATURATION = 50.0;
const SCORING_PREFETCH_FACTOR = 3;

export function decayScore(
  memory: MemoryRecord,
  lexical: number,
  weights = DECAY_WEIGHTS,
): number {
  const halfLife = memory.decay_half_life_days ?? DEFAULT_HALF_LIFE_DAYS;
  const lastUsedStr = memory.last_accessed_at ?? memory.created_at;
  let recency = 0;
  if (lastUsedStr) {
    const ageDays = Math.max(0, (Date.now() - new Date(lastUsedStr).getTime()) / 86400000);
    recency = Math.exp(-Math.LN2 * ageDays / Math.max(halfLife, 0.01));
  }
  const importance = (memory.importance_score ?? 0) / 10;
  const access = Math.log1p(memory.access_count ?? 0) / Math.log1p(ACCESS_SATURATION);
  const lexNorm = Math.max(0, Math.min(1, lexical));
  return (
    weights.importance * importance +
    weights.recency * recency +
    weights.access * Math.min(access, 1) +
    weights.lexical * lexNorm
  );
}

// ─── FTS helpers ──────────────────────────────────────────────────────────────

function buildFtsQuery(query: string): string | null {
  const stopWords = new Set(['the', 'and', 'for', 'with', 'this', 'that', 'about', 'before', 'after']);
  const tokens = [
    ...new Set(
      (query.toLowerCase().match(/[a-z0-9_]{2,}/g) ?? []).filter(t => !stopWords.has(t))
    ),
  ].slice(0, 16);
  return tokens.length > 0 ? tokens.join(' OR ') : null;
}

function fallbackSearch(
  db: DatabaseSync,
  conditions: string[],
  params: (string | number)[],
  limit: number,
): MemoryRow[] {
  const sql = `
    SELECT m.*, 0 AS _bm25
    FROM agent_memories m
    WHERE ${conditions.join(' AND ')}
    ORDER BY m.importance_score DESC, m.created_at DESC
    LIMIT ?
  `;
  return db.prepare(sql).all(...params, limit) as unknown as MemoryRow[];
}

export function lexicalSearch(
  db: DatabaseSync,
  query: string,
  limit: number,
  minImportance: number,
  tags: string[],
  labels: string[],
  states: string[],
): MemoryRecord[] {
  const ftsQuery = query ? buildFtsQuery(query) : null;
  const params: (string | number)[] = [];
  const conditions: string[] = [
    'm.importance_score >= ?',
    `m.state IN (${states.map(() => '?').join(',')})`,
  ];
  params.push(minImportance, ...states);

  if (labels.length > 0) {
    conditions.push(`m.label IN (${labels.map(() => '?').join(',')})`);
    params.push(...labels);
  }
  for (const tag of tags) {
    conditions.push('m.tags_text LIKE ?');
    params.push(`%,${tag},%`);
  }

  let rows: MemoryRow[];
  if (ftsQuery && hasFts(db)) {
    try {
      const sql = `
        SELECT m.*, ABS(bm25(memory_fts)) AS _bm25
        FROM agent_memories m
        JOIN memory_fts ON memory_fts.memory_id = m.memory_id
        WHERE memory_fts MATCH ?
          AND ${conditions.join(' AND ')}
        ORDER BY _bm25 DESC
        LIMIT ?
      `;
      rows = db.prepare(sql).all(ftsQuery, ...params, limit) as unknown as MemoryRow[];
    } catch {
      rows = fallbackSearch(db, conditions, params, limit);
    }
  } else {
    rows = fallbackSearch(db, conditions, params, limit);
  }

  const maxBm25 = rows.reduce((m, r) => Math.max(m, r._bm25 ?? 0), 0);
  return rows.map(row => {
    const lexical = maxBm25 > 0 ? (row._bm25 ?? 0) / maxBm25 : 0.5;
    const mem = rowToMemory(row);
    mem.score = decayScore(mem, lexical);
    return mem;
  });
}

// ─── bumpAccess ───────────────────────────────────────────────────────────────

export function bumpAccess(db: DatabaseSync, memoryIds: string[]): void {
  if (memoryIds.length === 0) return;
  const now = utcNow();
  const placeholders = memoryIds.map(() => '?').join(',');
  db.prepare(`
    UPDATE agent_memories
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
    importanceScore,
    label,
    tags = [],
    tagsCsv = '',
    references = [],
    supersedes = [],
    failureSignature = null,
    validFrom: vf,
    validTo: vt,
    workspacePath,
    repo: repoArg,
    ref: refArg,
    file: fileArg,
    fileTreeFingerprint = null,
    cwd,
  } = params;

  const imp = Number(importanceScore);
  if (!Number.isInteger(imp) || imp < 1 || imp > 10) {
    throw new Error(`importanceScore must be 1–10, got ${String(importanceScore)}`);
  }

  const memoryId = 'mem_' + randomUUID().replace(/-/g, '');
  const tagList = normalizeTags(tags, tagsCsv);
  const refList = normalizeReferences(references);
  const normalizedLabel = normalizeLabel(Array.isArray(label) ? label[0] : label);
  const createdAt = utcNow();
  const validFromVal = vf ?? createdAt;
  const memFile = normalizeFilePath(fileArg);

  const scope = fillScope(
    { workspace_path: workspacePath ?? null, repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );

  db.prepare(`
    INSERT INTO agent_memories (
      memory_id, agent_id, task_context, observation, importance_score,
      label, tags_json, tags_text, references_json, workspace_path, repo, ref,
      file_tree_fingerprint, file, created_at, updated_at,
      last_accessed_at, access_count, failure_signature, valid_from, valid_to
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)
  `).run(
    memoryId, agentId, taskContext, observation, imp,
    normalizedLabel, JSON.stringify(tagList), tagsText(tagList), JSON.stringify(refList),
    scope.workspace_path, scope.repo, scope.ref,
    fileTreeFingerprint, memFile, createdAt, createdAt,
    createdAt, failureSignature ?? null, validFromVal, vt ?? null
  );

  if (hasFts(db)) {
    db.prepare(
      'INSERT INTO memory_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)'
    ).run(
      memoryId, taskContext, observation,
      ftsTermsForRow({
        tags_json: JSON.stringify(tagList),
        references_json: JSON.stringify(refList),
        label: normalizedLabel,
        file: memFile,
        failure_signature: failureSignature ?? null,
        workspace_path: scope.workspace_path,
        repo: scope.repo,
        ref: scope.ref,
      })
    );
  }

  // Supersede old memories
  const superseded: string[] = [];
  for (const oldId of supersedes) {
    const r = db.prepare(`
      UPDATE agent_memories
      SET state = 'SUPERSEDED', superseded_by = ?, updated_at = ?,
          valid_to = COALESCE(valid_to, ?), expired_at = ?
      WHERE memory_id = ? AND memory_id <> ?
    `).run(memoryId, createdAt, validFromVal, createdAt, oldId, memoryId) as { changes: number };
    if (r.changes) superseded.push(oldId);
  }

  return {
    memoryId,
    memory: {
      memory_id: memoryId,
      agent_id: agentId,
      task_context: taskContext,
      observation,
      importance_score: imp,
      label: normalizedLabel,
      tags: tagList,
      references: refList,
      workspace_path: scope.workspace_path,
      repo: scope.repo,
      ref: scope.ref,
      file: memFile,
      state: 'ACTIVE' as const,
      created_at: createdAt,
    },
    superseded,
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
    states: statesRaw,
    sort = 'smart',
    globalOnly = false,
    asOf,
  } = params;

  const limit = Math.min(20, Math.max(1, Number(limitRaw) || 3));
  let minImportance = Math.max(1, Number(minImpRaw) || 1);
  if (smart === true || smart === 'true') minImportance = Math.max(1, minImportance - 1);

  const states = statesRaw ?? ['ACTIVE'];
  const labels = label
    ? (Array.isArray(label) ? label.map(normalizeLabel) : [normalizeLabel(label)])
    : [];

  let memories = lexicalSearch(
    db, query, limit * SCORING_PREFETCH_FACTOR, minImportance, tags, labels, states
  );

  // Normalize the caller's workspace to the same repo root that insertMemory stores
  // (fillScope), so recall from a subdirectory of a git repo matches memories whose
  // workspace_path was resolved to the repo root. Falls back to the raw path when the
  // caller is outside any git repo. Without this, an agent recording with cwd in a
  // subdirectory and recalling with the same cwd would filter out its own memory.
  let scope = workspacePath;
  if (!globalOnly && scope) {
    scope = fillScope({ workspace_path: null }, scope).workspace_path ?? scope;
    memories = memories.filter(m => !m.workspace_path || m.workspace_path === scope);
  }

  if (asOf) {
    const asOfDate = new Date(asOf);
    memories = memories.filter(m => {
      const vf = m.valid_from ? new Date(m.valid_from) : null;
      const vt = m.valid_to ? new Date(m.valid_to) : null;
      return (!vf || vf <= asOfDate) && (!vt || vt > asOfDate);
    });
  }

  memories.sort((a, b) => (b.score ?? 0) - (a.score ?? 0));
  memories = memories.slice(0, limit);
  bumpAccess(db, memories.map(m => m.memory_id));

  return {
    count: memories.length,
    memories,
    mode: hasFts(db) ? 'lexical' : 'fallback',
    sort,
    as_of: asOf ?? null,
    global_only: Boolean(globalOnly),
    states,
  };
}
