import type { AgentRecord,AgentStatus,LiteMessage,MemoryItem,PruneResult } from '@octocodeai/octocode-shared/entities';
import { generateAgentName } from './agent-naming.js';
import { bytesToEmbedding,cosineSimilarity,embeddingToBytes,isEmbeddingEnabled,runHostEmbedder } from '@octocodeai/octocode-shared/embed';
import { CoordinationState } from './coordination-state.js';
import { countPresentAgentPresence, countStaleAgentPresence } from './coordination-agent-presence.js';
import { agentFromCanonicalRow,CanonicalAgentRow,CanonicalMemoryRow,CanonicalMessageRow,cutoffIso,DEFAULT_SEMANTIC_MIN_SIMILARITY,memoryFromCanonicalRow,messageFromCanonicalSignalRow,now,parseMetadata,required,splitFiles,splitTags } from './coordination-shared.js';
import { insertMemory } from '../memory-write.js';
import { forgetMemory as forgetCanonicalMemory } from '../memory-lifecycle.js';
import { getMemory as getCanonicalMemory } from '../memory-recall.js';
import { recallMemory as recallCanonicalMemory } from '../memory-semantic.js';
import { insertNotification } from '../notifications-core.js';
import { deletePrunableSignals } from '../notifications-signals.js';
import { canonicalizePath } from '../git.js';
import {
  containsSecretLikeText,
  MEMORY_EVALUATION_CORPUS_V1,
  runMemoryEvaluationCorpus,
  type MemoryEvaluationCorpusV1,
  type MemoryEvaluationReportV1,
  type MemoryRecallModeV1,
} from '../memory-hardening.js';

export interface VerifiedMemoryV1 {
  version: 1;
  memoryId: string;
  label: string;
  text: string;
  scope: 'project' | 'artifact';
  sourceDigest: string;
  verifiedAt: string;
  validUntil?: string;
  importance: number;
  explanation?: string;
}

export abstract class CoordinationMemoryAgents extends CoordinationState {
  protected get canonicalWorkspace(): string {
    return canonicalizePath(this.workspace);
  }

  storeVerifiedMemory(params: { label: string; text: string; scope?: 'project' | 'artifact'; sourceDigest: string; verifiedAt?: string; validUntil?: string; importance?: number; tags?: string | string[] | null }): VerifiedMemoryV1 {
    const label = required(params.label, 'label');
    const text = required(params.text, 'text');
    const sourceDigest = required(params.sourceDigest, 'sourceDigest');
    if (containsSecretLikeText(`${label}\n${text}`)) throw new Error('memory rejected: secret-like content must never enter durable memory');
    const verifiedAt = params.verifiedAt ?? now();
    if (!Number.isFinite(Date.parse(verifiedAt))) throw new Error('verifiedAt must be an ISO timestamp');
    if (params.validUntil && !Number.isFinite(Date.parse(params.validUntil))) throw new Error('validUntil must be an ISO timestamp');
    const importance = Math.min(Math.max(Math.trunc(params.importance ?? 5), 1), 10);
    const memoryId = this.writeTransaction(() => {
      const inserted = insertMemory(this.db, {
        agentId: 'awareness',
        taskContext: label,
        observation: text,
        importance,
        label,
        tags: splitTags(params.tags),
        workspacePath: this.canonicalWorkspace,
        validFrom: verifiedAt,
        validTo: params.validUntil,
      });
      this.db.prepare(`UPDATE awareness_memories
        SET scope_kind = ?, source_digest = ?, verified_at = ?, secret_scan_status = 'passed'
        WHERE memory_id = ?`).run(params.scope ?? 'project', sourceDigest, verifiedAt, inserted.memoryId);
      return inserted.memoryId;
    });
    this.embedMemory(memoryId, `${label}\n${text}`);
    return { version: 1, memoryId, label, text, scope: params.scope ?? 'project', sourceDigest, verifiedAt, ...(params.validUntil ? { validUntil: params.validUntil } : {}), importance };
  }

  recallVerifiedMemory(params: { query?: string; label?: string; sourceDigest?: string; scope?: 'project' | 'artifact'; limit?: number; now?: string; mode?: MemoryRecallModeV1; minSimilarity?: number } = {}): VerifiedMemoryV1[] {
    const stamp = params.now ?? now();
    const clauses = ["workspace_path = ?", "state = 'ACTIVE'", 'verified_at IS NOT NULL', "secret_scan_status = 'passed'", '(valid_to IS NULL OR valid_to > ?)'];
    const values: Array<string | number> = [this.canonicalWorkspace, stamp];
    if (params.label?.trim()) { clauses.push('label = ?'); values.push(params.label.trim()); }
    if (params.sourceDigest?.trim()) { clauses.push('source_digest = ?'); values.push(params.sourceDigest.trim()); }
    if (params.scope) { clauses.push('scope_kind = ?'); values.push(params.scope); }
    const limit = Math.min(Math.max(params.limit ?? 10, 1), 50);
    const query = params.query?.trim();
    const lexicalClauses = [...clauses];
    const lexicalValues: Array<string | number> = [...values];
    if (query) { lexicalClauses.push('(observation LIKE ? OR label LIKE ? OR tags_json LIKE ?)'); const like = `%${query}%`; lexicalValues.push(like, like, like); }
    const readRows = (sqlClauses: string[], sqlValues: Array<string | number>, sqlLimit = limit) => this.db.prepare(`SELECT * FROM awareness_memories WHERE ${sqlClauses.join(' AND ')} ORDER BY importance DESC, verified_at DESC LIMIT ?`).all(...sqlValues, sqlLimit) as Array<Record<string, unknown>>;
    const toMemory = (row: Record<string, unknown>, similarity?: number): VerifiedMemoryV1 => ({
      version: 1,
      memoryId: String(row['memory_id']), label: String(row['label']), text: String(row['observation']),
      scope: row['scope_kind'] === 'artifact' ? 'artifact' : 'project', sourceDigest: String(row['source_digest']),
      verifiedAt: String(row['verified_at']), ...(row['valid_to'] ? { validUntil: String(row['valid_to']) } : {}),
      importance: Number(row['importance'] ?? 5),
      explanation: `verified memory; scope=${String(row['scope_kind'] ?? 'project')}; source=${String(row['source_digest'])}${similarity === undefined ? '' : `; similarity=${similarity.toFixed(4)}`}`,
    });
    const lexical = (): VerifiedMemoryV1[] => readRows(lexicalClauses, lexicalValues).map((row) => toMemory(row));
    const mode = params.mode ?? 'lexical';
    if (!query || mode === 'lexical') return lexical();
    if (!isEmbeddingEnabled()) return lexical();
    // Refresh missing, cross-model, or dimension-mismatched vectors before
    // ranking. Any host failure leaves semantic empty and safely falls back.
    this.reindexMemories();
    let queryVec: Float32Array;
    let queryModel: string;
    try { const embedded = runHostEmbedder(query); queryVec = embedded.embedding; queryModel = embedded.model; } catch { return lexical(); }
    const semanticClauses = [...clauses, 'embedding IS NOT NULL', 'embedding_model = ?'];
    const semanticValues: Array<string | number> = [...values, queryModel];
    const semantic = readRows(semanticClauses, semanticValues, 2000)
      .flatMap((row) => {
        try {
          const sim = cosineSimilarity(queryVec, bytesToEmbedding(row['embedding'] as Uint8Array));
          return sim > 0 && sim >= Math.min(Math.max(params.minSimilarity ?? DEFAULT_SEMANTIC_MIN_SIMILARITY, 0), 1) ? [{ row, sim }] : [];
        } catch { return []; }
      })
      .sort((a, b) => b.sim - a.sim)
      .slice(0, limit)
      .map(({ row, sim }) => toMemory(row, sim));
    if (mode === 'semantic') return semantic.length ? semantic : lexical();
    const combined = [...semantic];
    const seen = new Set(combined.map((item) => item.memoryId));
    for (const item of lexical()) if (!seen.has(item.memoryId) && combined.length < limit) combined.push(item);
    return combined;
  }

  evaluateVerifiedMemory(params: { corpus?: MemoryEvaluationCorpusV1; now?: string; limit?: number; minSimilarity?: number } = {}): MemoryEvaluationReportV1 {
    return runMemoryEvaluationCorpus(params.corpus ?? MEMORY_EVALUATION_CORPUS_V1, (item) => this.recallVerifiedMemory({
      query: item.query,
      mode: item.mode,
      scope: item.scope,
      now: params.now,
      limit: params.limit,
      minSimilarity: params.minSimilarity,
    }));
  }

  storeMemory(params: { label: string; text: string; tags?: string | string[] | null }): MemoryItem {
    const label = required(params.label, 'label');
    const text = required(params.text, 'text');
    if (containsSecretLikeText(`${label}\n${text}`)) throw new Error('memory rejected: secret-like content must never enter durable memory');
    const inserted = insertMemory(this.db, {
      agentId: 'awareness',
      taskContext: label,
      observation: text,
      importance: 5,
      label,
      tags: splitTags(params.tags),
      workspacePath: this.canonicalWorkspace,
    });
    const memoryId = inserted.memoryId;
    // Best-effort: embed on write when a host embedder is configured. Never blocks the store.
    this.embedMemory(memoryId, `${label}\n${text}`);
    return {
      memoryId,
      label: inserted.memory.label,
      text: inserted.memory.observation,
      tags: inserted.memory.tags,
      createdAt: inserted.memory.created_at,
    };
  }

  /** Compute + persist an embedding for one memory; silently no-ops when disabled or on failure. */
  protected embedMemory(memoryId: string, text: string): boolean {
    if (!isEmbeddingEnabled()) return false;
    try {
      const { embedding, model } = runHostEmbedder(text);
      this.db.prepare('UPDATE awareness_memories SET embedding = ?, embedding_model = ? WHERE memory_id = ?')
        .run(embeddingToBytes(embedding), model, memoryId);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Backfill embeddings for memories missing them (or all when force). Returns
   * how many were (re)embedded. No-op with embedded:0 when no host embedder.
   */
  reindexMemories(params: { force?: boolean; limit?: number } = {}): { enabled: boolean; scanned: number; embedded: number } {
    if (!isEmbeddingEnabled()) return { enabled: false, scanned: 0, embedded: 0 };
    const limit = Math.min(Math.max(params.limit ?? 500, 1), 5000);
    let model: string;
    let bytes: number;
    try { const probe = runHostEmbedder('octocode memory embedding compatibility probe'); model = probe.model; bytes = probe.embedding.byteLength; } catch { return { enabled: true, scanned: 0, embedded: 0 }; }
    const where = params.force
      ? ' WHERE workspace_path = ?'
      : ' WHERE workspace_path = ? AND (embedding IS NULL OR embedding_model IS NULL OR embedding_model != ? OR length(embedding) != ?)';
    const rows = this.db.prepare(`SELECT memory_id, label, observation FROM awareness_memories${where} ORDER BY created_at DESC LIMIT ?`)
      .all(...(params.force ? [this.canonicalWorkspace, limit] : [this.canonicalWorkspace, model, bytes, limit])) as Array<{ memory_id: string; label: string; observation: string }>;
    let embedded = 0;
    for (const row of rows) if (this.embedMemory(row.memory_id, `${row.label}\n${row.observation}`)) embedded++;
    return { enabled: true, scanned: rows.length, embedded };
  }

  forgetMemory(params: { memoryId: string }): { forgotten: boolean } {
    const result = forgetCanonicalMemory(this.db, {
      memoryIds: [required(params.memoryId, 'memory-id')],
      workspacePath: this.canonicalWorkspace,
      dryRun: false,
    });
    return { forgotten: result.deleted > 0 };
  }

  recallMemory(params: { query?: string | null; label?: string | null; limit?: number; semantic?: boolean; minSimilarity?: number } = {}): MemoryItem[] {
    const limit = Math.min(Math.max(params.limit ?? 10, 1), 50);
    const common = {
      query: params.query?.trim() ?? '',
      ...(params.label?.trim() ? { label: params.label.trim() } : {}),
      limit,
      workspacePath: this.canonicalWorkspace,
      strictScope: true,
    };
    const result = params.semantic
      ? recallCanonicalMemory(this.db, common, true)
      : getCanonicalMemory(this.db, common);
    return ((result['memories'] ?? []) as CanonicalMemoryRow[]).map(memoryFromCanonicalRow);
  }

  pruneMemories(params: { olderThanMs: number; label?: string | null; dryRun?: boolean }): PruneResult {
    const olderThan = cutoffIso(params.olderThanMs);
    const label = params.label?.trim();
    const clauses = ['workspace_path = ?', 'created_at < ?'];
    const values: string[] = [this.canonicalWorkspace, olderThan];
    if (label) {
      clauses.push('label = ?');
      values.push(label);
    }
    const dryRun = params.dryRun !== false;
    const rows = this.db.prepare(`SELECT memory_id FROM awareness_memories WHERE ${clauses.join(' AND ')}`)
      .all(...values) as Array<{ memory_id: string }>;
    const ids = rows.map((row) => row.memory_id);
    if (ids.length === 0) return { dryRun, matched: 0, deleted: 0, olderThan };
    const result = forgetCanonicalMemory(this.db, {
      memoryIds: ids,
      workspacePath: this.canonicalWorkspace,
      dryRun,
    });
    return { dryRun, matched: dryRun ? result.would_delete ?? 0 : result.deleted, deleted: result.deleted, olderThan };
  }

  protected getMemory(memoryId: string): MemoryItem {
    const row = this.db.prepare('SELECT * FROM awareness_memories WHERE workspace_path = ? AND memory_id = ?')
      .get(this.canonicalWorkspace, memoryId) as unknown as CanonicalMemoryRow | undefined;
    if (!row) throw new Error(`memory not found: ${memoryId}`);
    return memoryFromCanonicalRow(row);
  }

  protected getAgent(agentId: string): AgentRecord {
    const row = this.db.prepare('SELECT * FROM awareness_agents WHERE workspace_path = ? AND agent_id = ?')
      .get(this.canonicalWorkspace, agentId) as unknown as CanonicalAgentRow | undefined;
    if (!row) throw new Error(`agent not found: ${agentId}`);
    return agentFromCanonicalRow(row);
  }

  protected getMessage(messageId: string): LiteMessage {
    const row = this.db.prepare('SELECT s.*, NULL AS read_at FROM signals s WHERE s.workspace_path = ? AND s.signal_id = ?')
      .get(this.canonicalWorkspace, messageId) as unknown as CanonicalMessageRow | undefined;
    if (!row) throw new Error(`message not found: ${messageId}`);
    return messageFromCanonicalSignalRow(row);
  }

  protected countStaleAgents(staleAfterMs: number): number {
    return countStaleAgentPresence(this.db, this.canonicalWorkspace, staleAfterMs);
  }

  /** Count non-left agents seen in the caller's bounded presence window. */
  protected countPresentAgents(staleAfterMs: number): number {
    return countPresentAgentPresence(this.db, this.canonicalWorkspace, staleAfterMs);
  }

  joinAgent(params: { agentId: string; name?: string | null; role?: string | null; metadata?: string | Record<string, unknown> | null }): AgentRecord {
    const stamp = now();
    const agentId = required(params.agentId, 'agent-id');
    const existing = this.db.prepare('SELECT agent_name, metadata_json FROM awareness_agents WHERE workspace_path = ? AND agent_id = ?').get(this.canonicalWorkspace, agentId) as { agent_name: string | null; metadata_json: string } | undefined;
    const metadataJson = params.metadata === undefined && existing ? existing.metadata_json : JSON.stringify(parseMetadata(params.metadata));
    const name = params.name?.trim() || existing?.agent_name || generateAgentName();
    this.db.prepare(`INSERT INTO awareness_agents(agent_id, workspace_path, agent_name, role, status, metadata_json, registered_at, last_seen_at)
      VALUES (?, ?, ?, ?, 'ACTIVE', ?, ?, ?)
      ON CONFLICT(workspace_path, agent_id) DO UPDATE SET agent_name = COALESCE(excluded.agent_name, awareness_agents.agent_name),
        role = COALESCE(excluded.role, awareness_agents.role), status = 'ACTIVE', metadata_json = excluded.metadata_json,
        last_seen_at = excluded.last_seen_at`).run(
          agentId,
          this.canonicalWorkspace,
          name,
          params.role?.trim() || null,
          metadataJson,
          stamp,
          stamp,
        );
    return this.getAgent(agentId);
  }

  touchAgent(params: { agentId: string; status?: AgentStatus }): AgentRecord {
    const agentId = required(params.agentId, 'agent-id');
    const status = params.status ?? 'ACTIVE';
    const existing = this.db.prepare('SELECT * FROM awareness_agents WHERE workspace_path = ? AND agent_id = ?').get(this.canonicalWorkspace, agentId) as unknown as CanonicalAgentRow | undefined;
    if (!existing) {
      this.joinAgent({ agentId });
      if (status === 'ACTIVE') return this.getAgent(agentId);
    }
    this.db.prepare('UPDATE awareness_agents SET status = ?, last_seen_at = ? WHERE workspace_path = ? AND agent_id = ?')
      .run(status, now(), this.canonicalWorkspace, agentId);
    return this.getAgent(agentId);
  }

  leaveAgent(params: { agentId: string }): AgentRecord {
    return this.touchAgent({ agentId: params.agentId, status: 'LEFT' });
  }

  listAgents(params: { includeLeft?: boolean; staleAfterMs?: number } = {}): AgentRecord[] {
    const clauses: string[] = ['workspace_path = ?'];
    const values: string[] = [this.canonicalWorkspace];
    if (!params.includeLeft || params.staleAfterMs) clauses.push("status != 'LEFT'");
    if (params.staleAfterMs) {
      clauses.push('last_seen_at < ?');
      values.push(cutoffIso(params.staleAfterMs));
    }
    const where = ` WHERE ${clauses.join(' AND ')}`;
    const rows = this.db.prepare(`SELECT * FROM awareness_agents${where} ORDER BY last_seen_at DESC, agent_id ASC`).all(...values);
    return (rows as unknown as CanonicalAgentRow[]).map(agentFromCanonicalRow);
  }

  sendMessage(params: { fromAgentId: string; toAgentId?: string | null; topic?: string | null; text: string; files?: string | string[] | null }): LiteMessage {
    const fromAgentId = required(params.fromAgentId, 'from-agent-id');
    const toAgentId = params.toAgentId?.trim() || null;
    const topic = params.topic?.trim() || null;
    const messageText = required(params.text, 'text');
    const files = splitFiles(params.files);
    const signalId = this.writeTransaction(() => {
      this.touchAgent({ agentId: fromAgentId });
      return insertNotification(this.db, {
        agentId: fromAgentId,
        workspacePath: this.canonicalWorkspace,
        toAgent: toAgentId,
        kind: 'fyi',
        subject: topic ?? 'message',
        body: messageText,
        files,
        refIds: [],
        inReplyTo: null,
        importance: 5,
        cwd: this.canonicalWorkspace,
      }).signal_id;
    });
    return this.getMessage(signalId);
  }

  listMessages(params: { agentId?: string | null; includeRead?: boolean; topic?: string | null; limit?: number } = {}): LiteMessage[] {
    const limit = Math.min(Math.max(params.limit ?? 20, 1), 100);
    const agentId = params.agentId?.trim();
    const topic = params.topic?.trim();
    const clauses: string[] = ['s.workspace_path = ?'];
    const values: string[] = [this.canonicalWorkspace];
    let readAt = 'NULL AS read_at';
    if (agentId) {
      clauses.push('s.from_agent != ?');
      values.push(agentId);
      clauses.push('(s.to_agent IS NULL OR s.to_agent = ?)');
      values.push(agentId);
      readAt = '(SELECT r.read_at FROM signal_reads r WHERE r.signal_id = s.signal_id AND r.agent_id = ?) AS read_at';
      values.unshift(agentId);
      if (!params.includeRead) {
        clauses.push('NOT EXISTS (SELECT 1 FROM signal_reads r WHERE r.signal_id = s.signal_id AND r.agent_id = ?)');
        values.push(agentId);
      }
    }
    if (topic) {
      clauses.push('s.subject = ?');
      values.push(topic);
    }
    const where = clauses.length ? ` WHERE ${clauses.join(' AND ')}` : '';
    const rows = this.db.prepare(`SELECT s.*, ${readAt} FROM signals s${where} ORDER BY s.created_at DESC LIMIT ?`).all(...values, limit);
    return (rows as unknown as CanonicalMessageRow[]).map(messageFromCanonicalSignalRow);
  }

  markMessageRead(params: { messageId: string; agentId: string }): LiteMessage {
    const message = this.getMessage(required(params.messageId, 'message-id'));
    const agentId = required(params.agentId, 'agent-id');
    if (message.fromAgentId === agentId || (message.toAgentId && message.toAgentId !== agentId)) {
      throw new Error(`message ${message.messageId} is not addressed to ${agentId}`);
    }
    this.touchAgent({ agentId });
    this.db.prepare(`INSERT INTO signal_reads(signal_id, agent_id, read_at)
      VALUES (?, ?, ?)
      ON CONFLICT(signal_id, agent_id) DO UPDATE SET read_at = excluded.read_at`).run(message.messageId, agentId, now());
    return this.listMessages({ agentId, includeRead: true, limit: 100 }).find((item) => item.messageId === message.messageId) ?? this.getMessage(message.messageId);
  }

  pruneMessages(params: { olderThanMs: number; readOnly?: boolean; dryRun?: boolean }): PruneResult {
    const olderThan = cutoffIso(params.olderThanMs);
    const clauses = ['workspace_path = ?', 'created_at < ?', "status = 'resolved'"];
    const values: string[] = [this.canonicalWorkspace, olderThan];
    if (params.readOnly) clauses.push('EXISTS (SELECT 1 FROM signal_reads r WHERE r.signal_id = signals.signal_id)');
    const where = clauses.join(' AND ');
    const candidates = (this.db.prepare(`SELECT signal_id FROM signals WHERE ${where}`).all(...values) as Array<{ signal_id: string }>)
      .map((row) => row.signal_id);
    const dryRun = params.dryRun !== false;
    const pruned = this.writeTransaction(() => deletePrunableSignals(this.db, candidates, dryRun));
    return { dryRun, matched: pruned.signalIds.length, deleted: pruned.deleted, olderThan };
  }

}
