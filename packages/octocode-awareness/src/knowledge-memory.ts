import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { withSqliteBusyRetry } from './sqlite.js';
import { storeVerifiedMemory } from './coordination/verified-memory.js';
import { containsSecretLikeText } from './memory-hardening.js';
import { decodeMemoryContent } from './memory-content.js';
import { appendDomainEvent } from './event-outbox.js';
import { normalizeWorkspacePath } from './git.js';
import { prepareMemoryEvidence } from './memory-evidence.js';
import { knowledgeAnchorReference, normalizeAnchors } from './knowledge-anchor.js';
import { knowledgeGetSchema, knowledgeRevalidateSchema, knowledgeSetSchema, type KnowledgeGetInput, type KnowledgeMetadata, type KnowledgeSetInput } from './knowledge-contract.js';
import { projectKnowledge, recallKnowledge, type KnowledgeBinding, type KnowledgeNext, type KnowledgeResult, type KnowledgeRow } from './knowledge-recall.js';

function digest(value: unknown): string { return createHash('sha256').update(JSON.stringify(value)).digest('hex'); }

function writeTransaction<T>(db: DatabaseSync, action: () => T): T {
  const owns = !db.isTransaction;
  if (owns) withSqliteBusyRetry(() => db.exec('BEGIN IMMEDIATE'));
  else db.exec('SAVEPOINT knowledge_write');
  try {
    const result = action();
    db.exec(owns ? 'COMMIT' : 'RELEASE knowledge_write');
    return result;
  } catch (error) {
    if (owns) db.exec('ROLLBACK');
    else { db.exec('ROLLBACK TO knowledge_write'); db.exec('RELEASE knowledge_write'); }
    throw error;
  }
}

async function setKnowledge(db: DatabaseSync, binding: KnowledgeBinding, requested: KnowledgeSetInput): Promise<KnowledgeResult> {
  const workspace = normalizeWorkspacePath(binding.workspace)!;
  const input = { ...requested, anchors: normalizeAnchors(workspace, requested.anchors),
    evidence_refs: [...new Set(requested.evidence_refs ?? [])].sort(),
    ...(requested.applicability ? { applicability: { ...requested.applicability,
      files: normalizeAnchors(workspace, requested.applicability.files.map(value => ({ kind: 'file', value }))).map(anchor => anchor.value),
    } } : {}),
  };
  if (containsSecretLikeText(JSON.stringify({ input, binding }))) throw new Error('memory rejected: secret-like content must never enter durable memory');
  if (input.evidence_refs.some(ref => ref.startsWith('knowledge-'))) throw new Error('evidence_refs cannot forge knowledge index references');
  if (input.validity?.from && input.validity?.until && input.validity.from >= input.validity.until) throw new Error('validity.until must be after validity.from');
  const requestDigest = digest(input);
  const requestReference = `knowledge-request:${digest([binding.actorId, input.request_id])}`;
  const findRetry = () => db.prepare(`SELECT m.* FROM awareness_memories m JOIN memory_refs r ON r.memory_id = m.memory_id
      WHERE m.workspace_path = ? AND r.reference = ?`).get(workspace, requestReference) as KnowledgeRow | undefined;
  const prior = findRetry();
  if (prior) return decodeMemoryContent(String(prior.observation)).knowledge?.request_digest !== requestDigest
    ? { status: 'conflict', code: 'REQUEST_ID_REUSED' } : { status: 'replayed', memory: await projectKnowledge(prior, workspace) };
  let applicability: KnowledgeMetadata['applicability'];
  if (input.applicability) {
    if ('fingerprint' in input.applicability) applicability = input.applicability;
    else {
      if (db.isTransaction) throw new Error('Capture knowledge evidence before entering a write transaction');
      const captured = await prepareMemoryEvidence({ agentId: binding.actorId, taskContext: input.title, observation: input.lesson,
        importance: 5, label: 'DECISION', workspacePath: workspace, captureFingerprint: true,
        references: input.applicability.files.map(file => `file:${resolve(workspace, file)}`),
      }, workspace);
      if (db.isTransaction) throw new Error('Knowledge write transaction changed during evidence capture');
      applicability = { files: input.applicability.files, fingerprint: captured.fileTreeFingerprint! };
    }
  }
  const result = writeTransaction<KnowledgeResult & { row?: KnowledgeRow }>(db, () => {
    const retry = findRetry();
    if (retry) {
      if (decodeMemoryContent(String(retry.observation)).knowledge?.request_digest !== requestDigest) return { status: 'conflict', code: 'REQUEST_ID_REUSED' };
      return { status: 'replayed', row: retry };
    }
    const current = db.prepare(`SELECT m.* FROM awareness_memories m JOIN memory_refs r ON r.memory_id = m.memory_id
      WHERE m.workspace_path = ? AND m.state = 'ACTIVE' AND r.reference = ?`).all(workspace, `knowledge-key:${input.key}`) as KnowledgeRow[];
    if (current.length > 1) return { status: 'conflict', code: 'MULTIPLE_CURRENT_REVISIONS', current_revision: null };
    const old = current[0];
    if ((old ? String(old.memory_id) : null) !== input.expected_revision) return { status: 'conflict', code: 'REVISION_CONFLICT', current_revision: old ? String(old.memory_id) : null };
    const recordedAt = new Date().toISOString();
    const knowledge: KnowledgeMetadata = { version: 1, key: input.key, title: input.title, anchors: input.anchors,
      evidence_refs: input.evidence_refs, ...(applicability ? { applicability } : {}), ...(input.validity ? { validity: input.validity } : {}),
      request_id: input.request_id, request_digest: requestDigest, expected_revision: input.expected_revision,
      attribution: { actor_id: binding.actorId, ...(binding.sessionId ? { session_id: binding.sessionId } : {}), recorded_at: recordedAt, rationale_source: 'caller' },
    };
    const saved = storeVerifiedMemory({ db, canonicalWorkspace: workspace, writeTransaction: action => action(), embedMemory: () => false }, {
      label: 'DECISION', text: input.lesson, why: input.why, constraint: input.constraint, knowledge,
      sourceDigest: requestDigest, supersedes: old ? [String(old.memory_id)] : [],
      references: [`knowledge-key:${input.key}`, requestReference, ...input.anchors.map(knowledgeAnchorReference), ...input.evidence_refs],
    });
    appendDomainEvent(db, { workspace, actorId: binding.actorId, sessionId: binding.sessionId, eventType: 'memory.set', retentionClass: 'audit',
      aggregateKind: 'memory', aggregateId: saved.memoryId, aggregateRevision: saved.memoryId, createdAt: recordedAt,
      payload: { key: input.key, revision: saved.memoryId, expected_revision: input.expected_revision, request_id: input.request_id } });
    const row = db.prepare('SELECT * FROM awareness_memories WHERE memory_id = ?').get(saved.memoryId) as KnowledgeRow;
    return { status: old ? 'updated' : 'created', row };
  });
  const { row, ...output } = result;
  return { ...output, ...(row ? { memory: await projectKnowledge(row, workspace) } : {}) };
}

export async function executeKnowledgeMemory(db: DatabaseSync, binding: KnowledgeBinding, operation: 'memory.set' | 'memory.get' | 'memory.revalidate', input: unknown): Promise<KnowledgeResult & { ok: boolean }> {
  if (operation === 'memory.set') {
    const result = await setKnowledge(db, binding, knowledgeSetSchema.parse(input));
    return { ...result, ok: result.status !== 'conflict' };
  }
  const params = operation === 'memory.get' ? knowledgeGetSchema.parse(input) : knowledgeRevalidateSchema.parse(input);
  const result = await recallKnowledge(db, binding, operation, params);
  return { ...result, ok: result.status !== 'terminal_limit' };
}

export interface KnowledgeBriefing {
  memories: Array<Record<string, unknown>>;
  partial: boolean;
  next?: KnowledgeNext;
  terminal_limit?: { code: string; message: string };
}

export async function getKnowledgeBriefing(db: DatabaseSync, binding: KnowledgeBinding, params: { files?: string[]; query?: string; flow?: string; failure?: string; limit?: number } = {}): Promise<KnowledgeBriefing> {
  const input: KnowledgeGetInput = {
    anchors: normalizeAnchors(binding.workspace, [...(params.files ?? []).map(value => ({ kind: 'file' as const, value })),
      ...(params.flow ? [{ kind: 'flow' as const, value: params.flow }] : []), ...(params.failure ? [{ kind: 'failure' as const, value: params.failure }] : [])]),
    ...(params.query ? { query: params.query } : {}), limit: Math.min(5, Math.max(1, params.limit ?? 3)),
  };
  if (!input.anchors?.length) delete input.anchors;
  const result = await recallKnowledge(db, binding, 'memory.get', input);
  const memories: Array<Record<string, unknown>> = [];
  if (result.terminal_limit) return { memories, partial: true, terminal_limit: result.terminal_limit };
  const briefing = () => ({ memories, partial: !!result.partial || memories.length < (result.memories?.length ?? 0),
    ...((result.partial || memories.length < (result.memories?.length ?? 0)) ? { next: { call: { operation: 'memory.get' as const, params: input } } } : {}) });
  for (const memory of result.memories ?? []) {
    memories.push({ key: memory.key, revision: memory.revision, title: memory.title, lesson: memory.lesson.slice(0, 160),
      evidence: { state: memory.evidence.state, claim_verification: 'unverified' },
      next: { call: { operation: 'memory.get', params: { key: memory.key, revision: memory.revision } } } });
    if (Buffer.byteLength(JSON.stringify(briefing())) > 1000) { memories.pop(); break; }
  }
  const output = briefing();
  if (Buffer.byteLength(JSON.stringify(output)) > 1000) return { memories: [], partial: true,
    terminal_limit: { code: 'KNOWLEDGE_BRIEFING_FILTER_LIMIT', message: 'The complete filtered continuation exceeds the briefing budget; run memory.get with the original files, flow, failure, or query anchors.' } };
  return output;
}
