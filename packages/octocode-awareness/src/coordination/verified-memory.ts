import type { DatabaseSync } from 'node:sqlite';
import { createHash } from 'node:crypto';
import { isAbsolute, relative, resolve, sep } from 'node:path';
import { bytesToEmbedding, cosineSimilarity, isEmbeddingEnabled, runHostEmbedder } from '@octocodeai/agent-contracts/embed';
import { canonicalMemoryInstant } from '../memory-scoring.js';
import { insertPreparedMemory } from '../memory-write.js';
import { containsSecretLikeText, MEMORY_EVALUATION_CORPUS_V1, runMemoryEvaluationCorpus, type MemoryEvaluationCorpusV1, type MemoryEvaluationReportV1, type MemoryRecallModeV1 } from '../memory-hardening.js';
import { normalizeArtifact, normalizeLabel, normalizeReferences, normalizeTags } from '../helpers.js';
import { DEFAULT_SEMANTIC_MIN_SIMILARITY, splitTags, now } from './coordination-shared.js';
import { decodeMemoryContent, encodeMemoryContent, renderMemoryContent } from '../memory-content.js';
import { repositoryWorkspacePaths } from '../git.js';
import type {
  VerifiedMemoryHost,
  VerifiedMemoryPageV1,
  VerifiedMemoryRecallParams,
  VerifiedMemoryStoreParams,
  VerifiedMemoryV1,
} from './verified-memory-types.js';
export type {
  VerifiedMemoryHost,
  VerifiedMemoryPageV1,
  VerifiedMemoryPartialReason,
  VerifiedMemoryRecallParams,
  VerifiedMemoryStoreParams,
  VerifiedMemoryV1,
} from './verified-memory-types.js';

const MAX_TEXT = 4000;
const MAX_SOURCE_DIGEST = 512;
const MAX_LIMIT = 50;
const MAX_OFFSET = 1_000_000_000;
const SEMANTIC_CANDIDATE_LIMIT = 2000;
const MAX_PAGE_BYTES = 16 * 1024;
function boundedText(value: string, field: string, max: number): string {
  const text = value.trim();
  if (!text) throw new Error(`${field} is required`);
  if (text.length > max) throw new Error(`${field} exceeds the maximum length of ${max}`);
  return text;
}

function boundedLimit(value: number | undefined): number {
  const limit = value ?? 10;
  if (!Number.isInteger(limit) || limit < 1 || limit > MAX_LIMIT) throw new Error(`limit must be an integer between 1 and ${MAX_LIMIT}`);
  return limit;
}

function boundedOffset(value: number | undefined): number {
  const offset = value ?? 0;
  if (!Number.isInteger(offset) || offset < 0 || offset > MAX_OFFSET) throw new Error(`offset must be an integer between 0 and ${MAX_OFFSET}`);
  return offset;
}

function escapeLike(value: string): string {
  return value.replaceAll('\\', '\\\\').replaceAll('%', '\\%').replaceAll('_', '\\_');
}

function normalizeVerifiedArtifact(value: string | null | undefined): string | null {
  return value == null ? null : normalizeArtifact(boundedText(value, 'artifact', 256));
}

function normalizeFiles(file: string | string[] | null | undefined, workspace: string): string[] {
  const values = Array.isArray(file) ? file : file ? [file] : [];
  if (values.length > 20) throw new Error('file accepts at most 20 paths');
  return values.map((value) => {
    const raw = boundedText(String(value), 'file', 1024);
    const requested = raw.startsWith('file:') ? raw.slice(5) : raw;
    if (requested.includes('\0') || requested.includes('\\')) throw new Error('file contains an invalid path character');
    const absolute = resolve(workspace, requested);
    const local = relative(resolve(workspace), absolute);
    if (!local || local === '..' || local.startsWith(`..${sep}`) || isAbsolute(local)) throw new Error('file must stay within the opened workspace');
    return `file:${absolute}`;
  });
}

function normalizeHistoryRef(value: string | undefined): string | undefined {
  if (!value) return undefined;
  const ref = boundedText(value, 'historyRef', 512);
  return ref.startsWith('history:') ? ref : `history:${ref}`;
}

function assertHistoryReference(db: DatabaseSync, workspace: string, reference: string | undefined): void {
  if (!reference) return;
  const operationId = reference.slice('history:'.length);
  if (!operationId) throw new Error('historyRef must identify an existing history operation in this workspace');
  try {
    const row = db.prepare('SELECT 1 AS present FROM local_history_operations WHERE operation_id = ? AND workspace_path = ?').get(operationId, workspace) as { present?: number } | undefined;
    if (row?.present !== 1) throw new Error('historyRef must identify an existing history operation in this workspace');
  } catch (error) {
    if (error instanceof Error && error.message.includes('no such table')) throw new Error('historyRef requires the existing local history store');
    throw error;
  }
}

function verifiedReferences(params: VerifiedMemoryStoreParams, workspace: string): string[] {
  const historyRef = normalizeHistoryRef(params.historyRef);
  const refs = [...normalizeFiles(params.file, workspace), ...(historyRef ? [historyRef] : [])];
  if (refs.length > 20) throw new Error('verified memory references exceed the maximum of 20');
  return normalizeReferences(refs);
}

function rowReferences(db: DatabaseSync, memoryId: string): string[] {
  try {
    return (db.prepare('SELECT reference FROM memory_refs WHERE memory_id = ? ORDER BY ordinal').all(memoryId) as Array<{ reference: string }>).map(row => row.reference);
  } catch (error) {
    if (error instanceof Error && error.message.includes('no such table')) return [];
    throw error;
  }
}

function historyEvidence(db: DatabaseSync, workspace: string, reference: string): VerifiedMemoryV1['historyEvidence'] {
  const operationId = reference.slice('history:'.length);
  try {
    const operation = db.prepare(`SELECT status, after_commit_oid,
      (SELECT COUNT(*) FROM local_history_versions WHERE operation_id = local_history_operations.operation_id) AS version_count
      FROM local_history_operations WHERE operation_id = ? AND workspace_path = ?`).get(operationId, workspace);
    if (!operation) return { state: 'unavailable', reason: 'The referenced history operation is unavailable in its source workspace.' };
    if (operation.status !== 'complete') return { state: 'incomplete', reason: `History capture is ${String(operation.status)}; it has no executable byte-read continuation.` };
    const readable = db.prepare(`SELECT file_path,
      CASE
        WHEN after_status = 'captured' AND after_oid IS NOT NULL THEN 'after'
        WHEN before_status = 'captured' AND before_oid IS NOT NULL THEN 'before'
        ELSE NULL
      END AS side
      FROM local_history_versions
      WHERE operation_id = ?
      ORDER BY ordinal, file_path
      LIMIT 1`).get(operationId) as { file_path?: string; side?: 'before' | 'after' | null } | undefined;
    if (!operation.after_commit_oid || !Number(operation.version_count) || !readable?.file_path || !readable.side) {
      return { state: 'incomplete', reason: 'Completed history metadata has no readable captured file side.' };
    }
    const next = { call: { operation: 'history.read' as const, params: {
      operation_id: operationId,
      file: readable.file_path,
      side: readable.side,
      source_workspace: workspace,
    } } };
    return { state: 'recorded', reason: 'History metadata records a completed capture; inspect it to check byte availability. This does not establish current file freshness.', next };
  } catch (error) {
    if (error instanceof Error && error.message.includes('no such table')) return { state: 'unavailable', reason: 'The local history store is unavailable.' };
    throw error;
  }
}

function toVerified(db: DatabaseSync, row: Record<string, unknown>, similarity?: number): VerifiedMemoryV1 {
  const content = decodeMemoryContent(String(row.observation));
  const references = rowReferences(db, String(row.memory_id));
  const files = references.filter(value => value.startsWith('file:')).map(value => relative(String(row.workspace_path), value.slice(5)));
  const historyRef = references.find(value => value.startsWith('history:'));
  const scope = row.scope_kind === 'artifact' ? 'artifact' : 'project';
  return {
    version: 1,
    memoryId: String(row.memory_id),
    workspacePath: String(row.workspace_path),
    label: String(row.label),
    ...content,
    scope,
    ...(row.artifact ? { artifact: String(row.artifact) } : {}),
    sourceDigest: String(row.source_digest),
    verifiedAt: String(row.verified_at),
    ...(row.valid_to ? { validUntil: String(row.valid_to) } : {}),
    importance: Number(row.importance ?? 5),
    ...(files.length ? { file: files } : {}),
    ...(historyRef ? { historyRef, historyEvidence: historyEvidence(db, String(row.workspace_path), historyRef) } : {}),
    explanation: `verified memory; scope=${scope}; source=${String(row.source_digest)}${similarity === undefined ? '' : `; similarity=${similarity.toFixed(4)}`}`,
  };
}

export function storeVerifiedMemory(host: VerifiedMemoryHost, params: VerifiedMemoryStoreParams): VerifiedMemoryV1 {
  const label = normalizeLabel(boundedText(params.label, 'label', 128));
  const text = boundedText(params.text, 'text', MAX_TEXT);
  const sourceDigest = boundedText(params.sourceDigest, 'sourceDigest', MAX_SOURCE_DIGEST);
  const scope = params.scope ?? 'project';
  if (scope !== 'project' && scope !== 'artifact') throw new Error('scope must be project or artifact');
  const artifact = normalizeArtifact(params.artifact === undefined ? undefined : boundedText(params.artifact, 'artifact', 256));
  if (scope === 'artifact' && !artifact) throw new Error('artifact is required when scope is artifact');
  if (containsSecretLikeText(`${label}\n${text}\n${params.why ?? ''}\n${params.constraint ?? ''}`)) throw new Error('memory rejected: secret-like content must never enter durable memory');
  const verifiedAt = canonicalMemoryInstant(params.verifiedAt ?? now(), 'verifiedAt')!;
  const validUntil = params.validUntil === undefined ? undefined : canonicalMemoryInstant(params.validUntil, 'validUntil');
  if (validUntil != null && validUntil <= verifiedAt) throw new Error('valid_until must be after verified_at');
  const importance = params.importance ?? 5;
  if (!Number.isInteger(importance) || importance < 1 || importance > 10) throw new Error('importance must be an integer between 1 and 10');
  const supersedes = params.supersedes ?? [];
  if (supersedes.length > 200) throw new Error('supersedes accepts at most 200 memory ids');
  const normalizedSupersedes = supersedes.map(value => boundedText(String(value), 'supersedes', 128));
  const tags = normalizeTags(splitTags(params.tags)).sort();
  const observation = encodeMemoryContent({ text,
    ...(params.area ? { area: boundedText(params.area, 'area', 256) } : {}),
    ...(params.why ? { why: boundedText(params.why, 'why', 1000) } : {}),
    ...(params.constraint ? { constraint: boundedText(params.constraint, 'constraint', 1000) } : {}),
  });
  const references = verifiedReferences(params, host.canonicalWorkspace);
  if (Buffer.byteLength(JSON.stringify({ observation, tags, references, sourceDigest }), 'utf8') > 8192) throw new Error('verified memory exceeds the 8192-byte evidence budget');
  let result: { memory: VerifiedMemoryV1; inserted: boolean };
  result = host.writeTransaction(() => {
    assertHistoryReference(host.db, host.canonicalWorkspace, references.find(reference => reference.startsWith('history:')));
    const duplicate = host.db.prepare(`SELECT * FROM awareness_memories WHERE workspace_path = ? AND artifact IS ? AND scope_kind = ?
      AND label = ? AND observation = ? AND tags_json = ? AND source_digest = ? AND importance = ?
      AND (? IS NULL OR verified_at = ?) AND valid_to IS ? AND state = 'ACTIVE'
      AND (SELECT COUNT(*) FROM memory_refs r WHERE r.memory_id = awareness_memories.memory_id) = ?
      AND NOT EXISTS (SELECT 1 FROM memory_refs r WHERE r.memory_id = awareness_memories.memory_id
        AND r.reference NOT IN (SELECT value FROM json_each(?)))
      ORDER BY created_at ASC, memory_id ASC LIMIT 1`)
      .get(host.canonicalWorkspace, artifact ?? null, scope, label, observation, JSON.stringify(tags), sourceDigest, importance,
        params.verifiedAt === undefined ? null : verifiedAt, verifiedAt, validUntil ?? null, references.length, JSON.stringify(references));
    if (duplicate && normalizedSupersedes.length === 0) return { memory: toVerified(host.db, duplicate), inserted: false };
    const inserted = insertPreparedMemory(host.db, { agentId: 'awareness', taskContext: label, observation, importance, label, tags, references, supersedes: normalizedSupersedes, workspacePath: host.canonicalWorkspace, artifact, validFrom: verifiedAt, validTo: validUntil });
    host.db.prepare(`UPDATE awareness_memories SET scope_kind = ?, source_digest = ?, verified_at = ?, secret_scan_status = 'passed' WHERE memory_id = ?`)
      .run(scope, sourceDigest, verifiedAt, inserted.memoryId);
    const row = host.db.prepare('SELECT * FROM awareness_memories WHERE memory_id = ?').get(inserted.memoryId) as Record<string, unknown>;
    return { memory: toVerified(host.db, row), inserted: true };
  });
  if (result.inserted) host.embedMemory(result.memory.memoryId, `${label}\n${renderMemoryContent(observation)}`);
  return result.memory;
}

function baseClauses(host: VerifiedMemoryHost, params: VerifiedMemoryRecallParams, stamp: string): { clauses: string[]; values: Array<string | number> } {
  const scope = params.scope;
  const artifact = normalizeVerifiedArtifact(params.artifact);
  if (scope === 'artifact' && !artifact) throw new Error('artifact is required when scope is artifact');
  const workspaces = params.strictScope ? [host.canonicalWorkspace] : repositoryWorkspacePaths(host.canonicalWorkspace);
  const clauses = ["workspace_path IN (SELECT value FROM json_each(?))", "state = 'ACTIVE'", 'verified_at IS NOT NULL', "secret_scan_status = 'passed'", '(valid_from IS NULL OR valid_from <= ?)', '(valid_to IS NULL OR valid_to > ?)'];
  const values: Array<string | number> = [JSON.stringify(workspaces), stamp, stamp];
  if (scope) { clauses.push('scope_kind = ?'); values.push(scope); }
  if (artifact) { clauses.push('artifact = ?'); values.push(artifact); }
  if (params.memoryId) { clauses.push('memory_id = ?'); values.push(params.memoryId); }
  if (params.label?.trim()) { clauses.push('label = ?'); values.push(normalizeLabel(params.label)); }
  if (params.sourceDigest?.trim()) { clauses.push('source_digest = ?'); values.push(params.sourceDigest.trim()); }
  const files = normalizeFiles(params.file, host.canonicalWorkspace);
  for (const reference of files) {
    clauses.push("EXISTS (SELECT 1 FROM memory_refs mr_file WHERE mr_file.memory_id = awareness_memories.memory_id AND mr_file.reference = 'file:' || awareness_memories.workspace_path || ?)");
    values.push(`${sep}${relative(host.canonicalWorkspace, reference.slice(5))}`);
  }
  if (params.area?.trim()) {
    clauses.push("CASE WHEN json_valid(observation) THEN json_extract(observation, '$.area') ELSE NULL END = ?");
    values.push(boundedText(params.area, 'area', 256));
  }
  return { clauses, values };
}

function nextCall(params: VerifiedMemoryRecallParams, offset: number, stamp: string, revision: string): VerifiedMemoryPageV1['next'] {
  // Keep the validity instant stable across every page of one recall.
  const nextParams: VerifiedMemoryRecallParams = { offset, limit: params.limit ?? 10, now: stamp };
  nextParams.revision = revision;
  if (params.query !== undefined) nextParams.query = params.query;
  if (params.label !== undefined) nextParams.label = params.label;
  if (params.sourceDigest !== undefined) nextParams.sourceDigest = params.sourceDigest;
  if (params.scope !== undefined) nextParams.scope = params.scope;
  if (params.artifact !== undefined) nextParams.artifact = params.artifact;
  if (params.mode !== undefined) nextParams.mode = params.mode;
  if (params.minSimilarity !== undefined) nextParams.minSimilarity = params.minSimilarity;
  if (params.file !== undefined) nextParams.file = params.file;
  if (params.area !== undefined) nextParams.area = params.area;
  if (params.memoryId !== undefined) nextParams.memoryId = params.memoryId;
  if (params.strictScope !== undefined) nextParams.strictScope = params.strictScope;
  return { params: nextParams };
}

function memoryRevision(host: VerifiedMemoryHost, clauses: string[], values: Array<string | number>, params: VerifiedMemoryRecallParams, mode: MemoryRecallModeV1): string {
  const hash = createHash('sha256').update(JSON.stringify([host.canonicalWorkspace, mode, params.query?.trim() ?? '', params.minSimilarity ?? DEFAULT_SEMANTIC_MIN_SIMILARITY, clauses, values])).update('\n');
  const rows = host.db.prepare(`SELECT * FROM awareness_memories WHERE ${clauses.join(' AND ')} ORDER BY memory_id ASC`).iterate(...values) as Iterable<Record<string, unknown>>;
  for (const row of rows) {
    // Include presented provenance and evidence metadata, not only sort keys.
    hash.update(JSON.stringify(row)).update(JSON.stringify(toVerified(host.db, row))).update('\n');
  }
  return `verified-memory-v1:${hash.digest('hex')}`;
}

function pageMemories(candidates: VerifiedMemoryV1[], limit: number): VerifiedMemoryV1[] {
  const page: VerifiedMemoryV1[] = [];
  let bytes = 2;
  for (const candidate of candidates.slice(0, limit)) {
    const candidateBytes = Buffer.byteLength(JSON.stringify(candidate), 'utf8') + (page.length > 0 ? 1 : 0);
    if (page.length > 0 && bytes + candidateBytes > MAX_PAGE_BYTES) break;
    page.push(candidate);
    bytes += candidateBytes;
  }
  return page;
}

export function recallVerifiedMemory(host: VerifiedMemoryHost, params: VerifiedMemoryRecallParams = {}): VerifiedMemoryPageV1 {
  if (params.memoryId && params.query !== undefined) throw new Error('memory_id cannot be combined with query');
  const stamp = canonicalMemoryInstant(params.now ?? now(), 'now')!;
  const limit = boundedLimit(params.limit);
  const offset = boundedOffset(params.offset);
  const base = baseClauses(host, params, stamp);
  const query = params.query?.trim();
  const mode = params.mode ?? 'lexical';
  const revision = memoryRevision(host, base.clauses, base.values, params, mode);
  if ((offset > 0 && !params.revision) || (params.revision !== undefined && params.revision !== revision)) {
    return { memories: [], partial: true, partialReasons: ['snapshot_changed'], revision, next: nextCall(params, 0, stamp, revision) };
  }
  const lexicalClauses = [...base.clauses];
  const lexicalValues: Array<string | number> = [...base.values];
  if (query) { lexicalClauses.push("(observation LIKE ? ESCAPE '\\' OR label LIKE ? ESCAPE '\\' OR tags_json LIKE ? ESCAPE '\\')"); const like = `%${escapeLike(query)}%`; lexicalValues.push(like, like, like); }
  const order = 'ORDER BY importance DESC, verified_at DESC, memory_id ASC';
  const count = Number((host.db.prepare(`SELECT COUNT(*) AS count FROM awareness_memories WHERE ${lexicalClauses.join(' AND ')}`).get(...lexicalValues) as { count: number }).count ?? 0);
  const readRows = (extra: string[] = lexicalClauses, extraValues: Array<string | number> = lexicalValues, cap = limit, start = offset): Array<Record<string, unknown>> => host.db.prepare(`SELECT * FROM awareness_memories WHERE ${extra.join(' AND ')} ${order} LIMIT ? OFFSET ?`).all(...extraValues, cap, start) as Array<Record<string, unknown>>;
  if (!query || mode === 'lexical' || !isEmbeddingEnabled()) {
    const memories = pageMemories(readRows().map(row => toVerified(host.db, row)), limit);
    const partial = offset + memories.length < count;
    return { memories, partial, partialReasons: partial ? ['limit'] : [], revision, ...(partial ? { next: nextCall(params, offset + memories.length, stamp, revision) } : {}), ...(query && mode !== 'lexical' ? { warnings: ['semantic embedding is unavailable; fell back to lexical recall'] } : {}) };
  }
  let queryVec: Float32Array;
  let queryModel: string;
  try { const embedded = runHostEmbedder(query); queryVec = embedded.embedding; queryModel = embedded.model; } catch {
    const memories = pageMemories(readRows().map(row => toVerified(host.db, row)), limit);
    const partial = offset + memories.length < count;
    return { memories, partial, partialReasons: partial ? ['limit'] : [], revision, ...(partial ? { next: nextCall(params, offset + memories.length, stamp, revision) } : {}), warnings: ['semantic embedding failed; fell back to lexical recall'] };
  }
  // Semantic retrieval ranks every eligible verified row by vector similarity;
  // the lexical phrase filter above is only a hybrid/fallback signal.
  const semanticClauses = [...base.clauses, 'embedding IS NOT NULL', 'embedding_model = ?'];
  const semanticValues: Array<string | number> = [...base.values, queryModel];
  const semanticCount = Number((host.db.prepare(`SELECT COUNT(*) AS count FROM awareness_memories WHERE ${semanticClauses.join(' AND ')}`).get(...semanticValues) as { count: number }).count ?? 0);
  const rows = host.db.prepare(`SELECT * FROM awareness_memories WHERE ${semanticClauses.join(' AND ')} ${order} LIMIT ?`).all(...semanticValues, SEMANTIC_CANDIDATE_LIMIT) as Array<Record<string, unknown>>;
  const ranked = rows.flatMap(row => {
    try { const similarity = cosineSimilarity(queryVec, bytesToEmbedding(row.embedding as Uint8Array)); return similarity > 0 && similarity >= Math.min(Math.max(params.minSimilarity ?? DEFAULT_SEMANTIC_MIN_SIMILARITY, 0), 1) ? [{ row, similarity }] : []; } catch { return []; }
  }).sort((a, b) => b.similarity - a.similarity || String(a.row.memory_id).localeCompare(String(b.row.memory_id)));
  const semantic = ranked.slice(offset, offset + limit).map(({ row, similarity }) => toVerified(host.db, row, similarity));
  const terminal = semanticCount > SEMANTIC_CANDIDATE_LIMIT;
  const terminalLimit = terminal ? { code: 'MEMORY_SEMANTIC_LIMIT' as const, candidateLimit: SEMANTIC_CANDIDATE_LIMIT, message: `Semantic recall examined only the first ${SEMANTIC_CANDIDATE_LIMIT} candidates; full coverage is unavailable.` } : undefined;
  if (mode === 'semantic') {
    if (!ranked.length) {
      const memories = pageMemories(readRows().map(row => toVerified(host.db, row)), limit);
      const partial = offset + memories.length < count;
      const noCoverage = semanticCount > SEMANTIC_CANDIDATE_LIMIT;
      return { memories, partial: noCoverage || partial, partialReasons: noCoverage ? ['terminal-limit'] : partial ? ['limit'] : [], revision, ...(noCoverage ? { terminalLimit: { code: 'MEMORY_SEMANTIC_LIMIT' as const, candidateLimit: SEMANTIC_CANDIDATE_LIMIT, message: `Semantic recall examined only the first ${SEMANTIC_CANDIDATE_LIMIT} candidates; full coverage is unavailable.` } } : {}), ...(!noCoverage && partial ? { next: nextCall(params, offset + memories.length, stamp, revision) } : {}), warnings: ['semantic query produced no matches; fell back to lexical recall'] };
    }
    const memories = pageMemories(semantic, limit);
    const partial = terminal || offset + memories.length < ranked.length;
    return { memories, partial, partialReasons: partial ? [terminal ? 'terminal-limit' : 'limit'] : [], revision, ...(terminal ? { terminalLimit } : {}), ...(!terminal && partial ? { next: nextCall(params, offset + memories.length, stamp, revision) } : {}) };
  }
  // Build the hybrid union once per call, then page the stable union. Paging
  // each source independently would duplicate or drop rows at page boundaries.
  const lexicalRows = host.db.prepare(`SELECT * FROM awareness_memories WHERE ${lexicalClauses.join(' AND ')} ${order} LIMIT ?`).all(...lexicalValues, SEMANTIC_CANDIDATE_LIMIT) as Array<Record<string, unknown>>;
  const union: Array<{ row: Record<string, unknown>; similarity?: number }> = ranked.map(({ row, similarity }) => ({ row, similarity }));
  const seen = new Set(union.map(item => String(item.row.memory_id)));
  for (const row of lexicalRows) {
    const id = String(row.memory_id);
    if (!seen.has(id)) { union.push({ row }); seen.add(id); }
  }
  const memories = pageMemories(union.slice(offset, offset + limit).map(({ row, similarity }) => toVerified(host.db, row, similarity)), limit);
  const lexicalTerminal = count > SEMANTIC_CANDIDATE_LIMIT;
  const terminalHybrid = terminal || lexicalTerminal;
  const finalTerminal = terminalHybrid ? { code: 'MEMORY_SEMANTIC_LIMIT' as const, candidateLimit: SEMANTIC_CANDIDATE_LIMIT, message: `Hybrid recall examined at most ${SEMANTIC_CANDIDATE_LIMIT} candidates per source; full coverage is unavailable.` } : undefined;
  const partial = terminalHybrid || offset + memories.length < union.length;
  return { memories, partial, partialReasons: partial ? [terminalHybrid ? 'terminal-limit' : 'limit'] : [], revision, ...(terminalHybrid ? { terminalLimit: finalTerminal } : {}), ...(!terminalHybrid && partial ? { next: nextCall(params, offset + memories.length, stamp, revision) } : {}) };
}

export function evaluateVerifiedMemory(recall: (params: VerifiedMemoryRecallParams) => VerifiedMemoryPageV1, params: { corpus?: MemoryEvaluationCorpusV1; now?: string; limit?: number; minSimilarity?: number } = {}): MemoryEvaluationReportV1 {
  return runMemoryEvaluationCorpus(params.corpus ?? MEMORY_EVALUATION_CORPUS_V1, item => recall({ query: item.query, mode: item.mode, scope: item.scope, artifact: item.artifact, now: params.now, limit: params.limit, minSimilarity: params.minSimilarity }).memories);
}
