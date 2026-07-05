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
import { hasFts, ftsTermsForRow, replaceMemoryReferences } from './db.js';
import type {
  InsertMemoryParams, InsertMemoryResult, GetMemoryParams, GetMemoryResult,
  MemoryRow, MemoryRecord, ForgetMemoryParams, ForgetMemoryResult,
} from './types.js';

// ─── Decay / salience scoring ─────────────────────────────────────────────────

const DECAY_WEIGHTS = { importance: 0.25, recency: 0.30, access: 0.15, lexical: 0.30 };
const DEFAULT_HALF_LIFE_DAYS = 30.0;
const ACCESS_SATURATION = 50.0;
const SCORING_PREFETCH_FACTOR = 3;
const SIMILARITY_THRESHOLD = 0.45;
const SIMILARITY_PREFETCH = 12;

function textTokens(text: string): Set<string> {
  const stopWords = new Set(['the', 'and', 'for', 'with', 'this', 'that', 'about', 'before', 'after', 'from', 'into', 'when', 'what']);
  return new Set((text.toLowerCase().match(/[a-z0-9_:-]{3,}/g) ?? []).filter(t => !stopWords.has(t)));
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
): Array<{ memory_id: string; similarity: number }> {
  const queryTokens = textTokens(text);
  if (queryTokens.size === 0) return [];

  const candidates = lexicalSearch(
    db, text, SIMILARITY_PREFETCH, 1, [], [], ['ACTIVE']
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

  const similar = findSimilarMemories(db, `${taskContext} ${observation}`);
  const noveltyScore = Math.max(0, Math.min(1, 1 - (similar[0]?.similarity ?? 0)));
  const similarMemoryIds = similar.map(m => m.memory_id);

  db.prepare(`
    INSERT INTO agent_memories (
      memory_id, agent_id, task_context, observation, importance_score,
      label, tags_json, tags_text, references_json, workspace_path, repo, ref,
      file_tree_fingerprint, file, novelty_score, similar_memory_ids_json, created_at, updated_at,
      last_accessed_at, access_count, failure_signature, valid_from, valid_to
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)
  `).run(
    memoryId, agentId, taskContext, observation, imp,
    normalizedLabel, JSON.stringify(tagList), tagsText(tagList), JSON.stringify(refList),
    scope.workspace_path, scope.repo, scope.ref,
    fileTreeFingerprint, memFile, noveltyScore, JSON.stringify(similarMemoryIds), createdAt, createdAt,
    createdAt, failureSignature ?? null, validFromVal, vt ?? null
  );

  // Populate structured reference index (Python-compatible memory_references table)
  if (refList.length > 0) {
    try { replaceMemoryReferences(db, memoryId, refList); } catch { /* ignore if table missing */ }
  }

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
      failure_signature: failureSignature ?? null,
      novelty_score: noveltyScore,
      similar_memory_ids: similarMemoryIds,
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
    states: statesRaw,
    sort = 'smart',
    globalOnly = false,
    strictScope = false,
    asOf,
    references = [],
    regex = [],
    fileRegex = [],
    files = [],
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

  // Workspace scope filter
  let scope = workspacePath;
  if (!globalOnly && scope) {
    scope = fillScope({ workspace_path: null }, scope).workspace_path ?? scope;
    if (strictScope) {
      memories = memories.filter(m => m.workspace_path === scope);
    } else {
      memories = memories.filter(m => !m.workspace_path || m.workspace_path === scope);
    }
  }
  if (globalOnly) {
    memories = memories.filter(m => !m.workspace_path && !m.repo && !m.ref);
  }

  // Exact file filter
  if (files.length > 0) {
    const normFiles = new Set(files.map(f => normalizeFilePath(f) ?? f));
    memories = memories.filter(m => m.file != null && normFiles.has(m.file));
  }

  // Reference filter — use memory_references table when available, fall back to inline JSON
  if (references.length > 0) {
    const refSet = new Set(references);
    const fromTable = new Set<string>();
    try {
      for (const ref of references) {
        const rows = db.prepare(
          'SELECT memory_id FROM memory_references WHERE reference = ?'
        ).all(ref) as unknown as Array<{ memory_id: string }>;
        rows.forEach(r => fromTable.add(r.memory_id));
      }
      if (fromTable.size > 0) {
        memories = memories.filter(m => fromTable.has(m.memory_id));
      } else {
        memories = memories.filter(m => (m.references ?? []).some(r => refSet.has(r)));
      }
    } catch {
      memories = memories.filter(m => (m.references ?? []).some(r => refSet.has(r)));
    }
  }

  // Regex filter
  if (regex.length > 0 || fileRegex.length > 0) {
    const compileRegex = (pattern: string): RegExp => {
      try {
        return new RegExp(pattern);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`invalid regex ${JSON.stringify(pattern)}: ${message}`);
      }
    };
    const compiledRegex = regex.map(compileRegex);
    const compiledFileRegex = fileRegex.map(compileRegex);
    memories = memories.filter(m => {
      if (compiledFileRegex.length > 0) {
        const fv = m.file ?? '';
        if (!compiledFileRegex.every(re => re.test(fv))) return false;
      }
      if (compiledRegex.length > 0) {
        const haystack = [
          m.task_context, m.observation,
          ...(m.tags ?? []), ...(m.references ?? []),
          m.label, m.workspace_path, m.repo, m.ref, m.file, m.failure_signature,
        ].filter(Boolean).join(' ');
        if (!compiledRegex.every(re => re.test(haystack))) return false;
      }
      return true;
    });
  }

  if (asOf) {
    const asOfDate = new Date(asOf);
    memories = memories.filter(m => {
      const vf = m.valid_from ? new Date(m.valid_from) : null;
      const vt = m.valid_to ? new Date(m.valid_to) : null;
      return (!vf || vf <= asOfDate) && (!vt || vt > asOfDate);
    });
  }

  // Sort
  if (sort === 'importance') {
    memories.sort((a, b) =>
      (b.importance_score - a.importance_score) || ((b.score ?? 0) - (a.score ?? 0)));
  } else if (sort === 'recent') {
    memories.sort((a, b) => (b.created_at ?? '').localeCompare(a.created_at ?? ''));
  } else if (sort === 'accessed') {
    memories.sort((a, b) =>
      (b.last_accessed_at ?? b.created_at ?? '').localeCompare(a.last_accessed_at ?? a.created_at ?? ''));
  } else {
    memories.sort((a, b) => (b.score ?? 0) - (a.score ?? 0));
  }

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

// ─── forgetMemory ─────────────────────────────────────────────────────────────────

/**
 * Delete memories by id, tag, age, or importance ceiling.
 * dryRun=true returns the count without deleting anything.
 */
export function forgetMemory(db: DatabaseSync, params: ForgetMemoryParams): ForgetMemoryResult {
  const { memoryIds = [], tags = [], before, maxImportance, dryRun = false } = params;

  const conditions: string[] = [];
  const bindParams: (string | number)[] = [];

  if (memoryIds.length > 0) {
    conditions.push(`memory_id IN (${memoryIds.map(() => '?').join(',')})`);
    bindParams.push(...memoryIds);
  }
  if (tags.length > 0) {
    conditions.push(`(${tags.map(() => 'tags_text LIKE ?').join(' OR ')})`);
    bindParams.push(...tags.map(t => `%,${t},%`));
  }
  if (before) {
    conditions.push('created_at < ?');
    bindParams.push(before);
  }
  if (maxImportance != null) {
    conditions.push('importance_score <= ?');
    bindParams.push(maxImportance);
  }

  if (conditions.length === 0) {
    throw new Error('forgetMemory requires at least one filter: memoryIds, tags, before, or maxImportance');
  }

  const where = conditions.join(' AND ');
  const rows = db.prepare(
    `SELECT memory_id FROM agent_memories WHERE ${where}`
  ).all(...bindParams) as unknown as Array<{ memory_id: string }>;
  const ids = rows.map(r => r.memory_id);

  if (dryRun) {
    return { deleted: 0, dry_run: true, would_delete: ids.length, memory_ids: ids };
  }

  if (ids.length > 0) {
    const ph = ids.map(() => '?').join(',');
    db.prepare(`DELETE FROM agent_memories WHERE memory_id IN (${ph})`).run(...ids);
    if (hasFts(db)) {
      db.prepare(`DELETE FROM memory_fts WHERE memory_id IN (${ph})`).run(...ids);
    }
    try {
      db.prepare(`DELETE FROM memory_references WHERE memory_id IN (${ph})`).run(...ids);
    } catch { /* ignore if table missing */ }
  }

  return { deleted: ids.length, memory_ids: ids };
}

// ─── mineWeakness ─────────────────────────────────────────────────────────────

export interface WeaknessCluster {
  failure_signature: string;
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
  minCount?: number;
  limit?: number;
  cwd?: string;
}

/**
 * Cluster memories by failure_signature to surface recurring failure patterns.
 * Sorted by count × avg_importance so the most impactful patterns appear first.
 */
export function mineWeakness(db: DatabaseSync, params: MineWeaknessParams = {}): MineWeaknessResult {
  const { minCount = 2, limit = 20, cwd } = params;
  const wsPath = params.workspacePath
    ?? (cwd ? fillScope({ workspace_path: null }, cwd).workspace_path : null);

  const conditions: string[] = ["failure_signature IS NOT NULL", "state = 'ACTIVE'"];
  const bindParams: (string | number)[] = [];
  if (wsPath) { conditions.push('(workspace_path = ? OR workspace_path IS NULL)'); bindParams.push(wsPath); }
  if (params.agentId) { conditions.push('agent_id = ?'); bindParams.push(params.agentId); }

  type ClusterRow = { failure_signature: string; freq: number; avg_imp: number; score: number; ids: string; labels: string };
  const rows = db.prepare(`
    SELECT failure_signature,
           count(*) AS freq,
           avg(importance_score) AS avg_imp,
           count(*) * avg(importance_score) AS score,
           group_concat(memory_id, ',') AS ids,
           group_concat(DISTINCT label) AS labels
    FROM agent_memories
    WHERE ${conditions.join(' AND ')}
    GROUP BY failure_signature
    HAVING freq >= ?
    ORDER BY score DESC
    LIMIT ?
  `).all(...bindParams, minCount, limit) as unknown as ClusterRow[];

  const clusters: WeaknessCluster[] = rows.map(row => {
    const ids = row.ids.split(',');
    const rep = db.prepare(
      `SELECT observation FROM agent_memories WHERE memory_id IN (${ids.map(() => '?').join(',')})
       ORDER BY importance_score DESC LIMIT 1`
    ).get(...ids) as { observation: string } | undefined;
    return {
      failure_signature: row.failure_signature,
      count: row.freq,
      avg_importance: Math.round(row.avg_imp * 10) / 10,
      score: Math.round(row.score * 10) / 10,
      memory_ids: ids,
      representative: rep?.observation?.slice(0, 200) ?? '',
      labels: row.labels.split(',').filter(Boolean),
    };
  });

  type TotalRow = { sigs: number; mems: number };
  const totals = db.prepare(
    `SELECT count(DISTINCT failure_signature) AS sigs, count(*) AS mems
     FROM agent_memories WHERE failure_signature IS NOT NULL AND state = 'ACTIVE'`
  ).get() as unknown as TotalRow;

  return { ok: true, clusters, total_signatures: totals.sigs, total_memories: totals.mems };
}
