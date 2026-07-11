import type { DatabaseSync } from 'node:sqlite';
import { utcNow, normalizeReferences, normalizeFilePath, rowToMemory, parseJsonList } from './helpers.js';
import { hasFts } from './db.js';
import type { MemoryRow, MemoryRecord } from './types.js';
import { applyScopeConditions, BM25_DEGENERATE_MAX, BM25_SQUASH_K, buildFtsQuery, decayScore, fallbackSearch, LexicalScopeOptions } from './memory-scoring.js';

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
  // An explicit non-empty query that contains no searchable tokens is not the
  // same operation as empty-query browsing. Returning the unfiltered corpus
  // here would present unrelated memories as lexical matches.
  if (query.trim() && !ftsQuery) return [];
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
  } else {
    // Normal recall evaluates ACTIVE knowledge at the current instant. Explicit
    // SUPERSEDED audit queries remain available without requiring an --as-of.
    const now = utcNow();
    conditions.push(`(m.state <> 'ACTIVE' OR (
      (m.valid_from IS NULL OR m.valid_from <= ?)
      AND (m.valid_to IS NULL OR m.valid_to > ?)
    ))`);
    params.push(now, now);
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

export function attachMemoryReferences(db: DatabaseSync, memories: MemoryRecord[]): void {
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

export function compileRecallRegex(pattern: string): RegExp {
  try {
    return new RegExp(pattern);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`invalid regex ${JSON.stringify(pattern)}: ${message}`);
  }
}

export function intersectCandidateIds(current: Set<string> | null, next: Set<string>): Set<string> {
  if (current === null) return new Set(next);
  const out = new Set<string>();
  for (const id of current) if (next.has(id)) out.add(id);
  return out;
}

export function exactReferenceCandidateIds(db: DatabaseSync, references: string[]): Set<string> {
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

export function fileReferenceCandidates(files: string[], baseDir?: string | null): string[] {
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

export function anyReferenceCandidateIds(db: DatabaseSync, references: string[]): Set<string> {
  const refs = [...new Set(references.map(ref => String(ref ?? '').trim().slice(0, 512)).filter(Boolean))];
  if (refs.length === 0) return new Set();
  const rows = db.prepare(
    `SELECT DISTINCT memory_id
     FROM memory_refs
     WHERE reference IN (${refs.map(() => '?').join(',')})`
  ).all(...refs) as unknown as Array<{ memory_id: string }>;
  return new Set(rows.map(row => row.memory_id));
}

export function fileRegexCandidateIds(db: DatabaseSync, regexes: RegExp[]): Set<string> {
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

export function regexCandidateIds(db: DatabaseSync, regexes: RegExp[]): Set<string> {
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
