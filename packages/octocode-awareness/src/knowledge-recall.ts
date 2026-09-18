import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { decodeMemoryContent } from './memory-content.js';
import { normalizeWorkspacePath } from './git.js';
import { checkMemoryEvidence, createMemoryEvidenceBudget } from './memory-evidence.js';
import { matchingAnchorReferences, normalizeAnchors, type KnowledgeAnchor } from './knowledge-anchor.js';
import type { KnowledgeGetInput, KnowledgeMetadata } from './knowledge-contract.js';

export interface KnowledgeBinding { workspace: string; actorId: string; sessionId?: string }
export interface KnowledgeEvidence {
  state: 'fresh' | 'stale' | 'unknown'; reason: string; claim_verification: 'unverified';
  references: Array<{ reference: string; state: 'unchecked' }>;
}
export interface KnowledgeMemory {
  key: string; revision: string; title: string; lesson: string; why?: string; constraint?: string;
  anchors: KnowledgeAnchor[]; attribution: KnowledgeMetadata['attribution'];
  state: 'current' | 'superseded'; superseded_by?: string; matched_by: Array<KnowledgeAnchor | { kind: 'key' | 'query' | 'workspace'; value: string }>;
  evidence: KnowledgeEvidence; applicability?: KnowledgeMetadata['applicability']; validity?: KnowledgeMetadata['validity'];
}
export interface KnowledgeNext { call: { operation: 'memory.get' | 'memory.revalidate'; params: KnowledgeGetInput } }
export interface KnowledgeResult {
  status: string; memory?: KnowledgeMemory; memories?: KnowledgeMemory[]; review_queue?: KnowledgeMemory[];
  current_revision?: string | null; code?: string; partial?: boolean; snapshot?: string; next?: KnowledgeNext;
  terminal_limit?: { code: string; message: string };
}
export type KnowledgeRow = Record<string, unknown>;

type EvidenceBudget = ReturnType<typeof createMemoryEvidenceBudget>;

async function evidence(metadata: KnowledgeMetadata, workspace: string, budget: EvidenceBudget): Promise<KnowledgeEvidence> {
  const base = { claim_verification: 'unverified' as const, references: metadata.evidence_refs.map(reference => ({ reference, state: 'unchecked' as const })) };
  const stamp = new Date().toISOString();
  if ((metadata.validity?.from && stamp < metadata.validity.from) || (metadata.validity?.until && stamp >= metadata.validity.until)) {
    return { ...base, state: 'stale', reason: 'outside_declared_validity' };
  }
  if (!metadata.applicability) return { ...base, state: 'unknown', reason: 'no_declared_file_fingerprints' };
  const observed = await checkMemoryEvidence({ workspace_path: workspace,
    references: metadata.applicability.files.map(file => `file:${resolve(workspace, file)}`),
    file_tree_fingerprint: metadata.applicability.fingerprint,
  }, workspace, true, budget);
  return { ...base, state: observed.state, reason: observed.reason };
}

export async function projectKnowledge(row: KnowledgeRow, workspace: string, matched: KnowledgeMemory['matched_by'] = [], budget = createMemoryEvidenceBudget()): Promise<KnowledgeMemory> {
  const content = decodeMemoryContent(String(row.observation));
  const metadata = content.knowledge;
  if (!metadata) throw new Error('Knowledge revision has invalid metadata');
  return {
    key: metadata.key, revision: String(row.memory_id), title: metadata.title, lesson: content.text,
    ...(content.why ? { why: content.why } : {}), ...(content.constraint ? { constraint: content.constraint } : {}),
    anchors: metadata.anchors, attribution: metadata.attribution,
    state: row.state === 'ACTIVE' ? 'current' : 'superseded', ...(row.superseded_by ? { superseded_by: String(row.superseded_by) } : {}),
    matched_by: matched, evidence: await evidence(metadata, workspace, budget),
    ...(metadata.applicability ? { applicability: metadata.applicability } : {}), ...(metadata.validity ? { validity: metadata.validity } : {}),
  };
}

function selection(workspace: string, input: KnowledgeGetInput) {
  const clauses = ['workspace_path = ?', "EXISTS (SELECT 1 FROM memory_refs k WHERE k.memory_id = awareness_memories.memory_id AND k.kind = 'knowledge-key')"];
  const values: Array<string | number> = [resolve(workspace)];
  if (input.key) {
    clauses.push('EXISTS (SELECT 1 FROM memory_refs k WHERE k.memory_id = awareness_memories.memory_id AND k.reference = ?)');
    values.push(`knowledge-key:${input.key}`);
  }
  if (input.revision) { clauses.push('memory_id = ?'); values.push(input.revision); }
  else clauses.push("state = 'ACTIVE'");
  const anchors = normalizeAnchors(workspace, [...(input.anchors ?? []), ...(input.failure_signature ? [{ kind: 'failure' as const, value: input.failure_signature }] : [])]);
  const references = [...new Set(anchors.flatMap(matchingAnchorReferences))];
  if (references.length) {
    clauses.push('EXISTS (SELECT 1 FROM memory_refs a WHERE a.memory_id = awareness_memories.memory_id AND a.reference IN (SELECT value FROM json_each(?)))');
    values.push(JSON.stringify(references));
  }
  if (input.query) {
    clauses.push("(observation LIKE ? ESCAPE '\\' OR task_context LIKE ? ESCAPE '\\')");
    const query = `%${input.query.replaceAll('\\', '\\\\').replaceAll('%', '\\%').replaceAll('_', '\\_')}%`;
    values.push(query, query);
  }
  return { clauses, values, references };
}

export async function recallKnowledge(db: DatabaseSync, binding: KnowledgeBinding, operation: 'memory.get' | 'memory.revalidate', input: KnowledgeGetInput): Promise<KnowledgeResult> {
  const workspace = normalizeWorkspacePath(binding.workspace)!;
  const { clauses, values, references } = selection(workspace, input);
  const hash = createHash('sha256').update(JSON.stringify([workspace, clauses, values]));
  const candidates: KnowledgeRow[] = [];
  const offset = input.offset ?? 0;
  const limit = input.limit ?? 10;
  let count = 0;
  for (const row of db.prepare(`SELECT memory_id, observation, state, superseded_by FROM awareness_memories WHERE ${clauses.join(' AND ')} ORDER BY memory_id`).iterate(...values) as Iterable<KnowledgeRow>) {
    hash.update(JSON.stringify(row));
    if (count >= offset && candidates.length < limit) candidates.push(row);
    count++;
  }
  const snapshot = `knowledge-v1:${hash.digest('hex')}`;
  const next = (start: number): KnowledgeNext => ({ call: { operation, params: { ...input, offset: start, snapshot } } });
  const listKey = operation === 'memory.get' ? 'memories' : 'review_queue';
  if ((offset && !input.snapshot) || (input.snapshot && input.snapshot !== snapshot)) {
    const restart = { status: 'snapshot_changed', [listKey]: [], partial: true, snapshot, next: next(0) };
    if (Buffer.byteLength(JSON.stringify(restart)) + 16 > (input.byte_budget ?? 16 * 1024)) return {
      status: 'terminal_limit', [listKey]: [], partial: true, snapshot,
      terminal_limit: { code: 'KNOWLEDGE_CONTINUATION_EXCEEDS_BUDGET', message: 'The complete restart call exceeds byte_budget; increase the budget or narrow the filters.' },
    };
    return restart;
  }
  const budget = createMemoryEvidenceBudget();
  const memories: KnowledgeMemory[] = [];
  const makeResult = (): KnowledgeResult => ({ status: 'ok', [listKey]: memories, partial: offset + memories.length < count, snapshot,
    ...(offset + memories.length < count ? { next: next(offset + memories.length) } : {}) });
  for (const row of candidates) {
    const metadata = decodeMemoryContent(String(row.observation)).knowledge!;
    const matched: KnowledgeMemory['matched_by'] = [
      ...metadata.anchors.filter(anchor => references.includes(`knowledge-anchor:${anchor.kind}:${anchor.value}`)),
      ...(input.key ? [{ kind: 'key' as const, value: input.key }] : []),
      ...(input.query ? [{ kind: 'query' as const, value: input.query }] : []),
      ...(!references.length && !input.query && !input.key ? [{ kind: 'workspace' as const, value: workspace }] : []),
    ];
    memories.push(await projectKnowledge(row, workspace, matched, budget));
    // The executor adds the success flag after the domain page is assembled.
    if (Buffer.byteLength(JSON.stringify(makeResult())) + 16 > (input.byte_budget ?? 16 * 1024)) { memories.pop(); break; }
  }
  if (!memories.length && candidates.length) return { status: 'terminal_limit', [listKey]: [], partial: true, snapshot,
    terminal_limit: { code: 'KNOWLEDGE_ITEM_EXCEEDS_BUDGET', message: 'The next complete revision exceeds byte_budget; increase byte_budget (maximum 24576).' } };
  return makeResult();
}
