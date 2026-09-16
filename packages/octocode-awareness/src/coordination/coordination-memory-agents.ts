import type { AgentRecord,AgentStatus,LiteMessage,MemoryItem,PruneResult } from '../entities.js';
import { generateAgentName } from './agent-naming.js';
import { embeddingToBytes,isEmbeddingEnabled,runHostEmbedder } from '../embed.js';
import { CoordinationState } from './coordination-state.js';
import type { MemoryRecallPage } from './coordination-core.js';
import { encodeSignalBody } from '../signal-data.js';
import { countPresentAgentPresence, countStaleAgentPresence } from './coordination-agent-presence.js';
import { agentFromCanonicalRow,CanonicalAgentRow,CanonicalMemoryRow,CanonicalMessageRow,cutoffIso,memoryFromCanonicalRow,messageFromCanonicalSignalRow,now,parseMetadata,required,splitFiles,splitTags } from './coordination-shared.js';
import { insertPreparedMemoryWithSimilarityGate } from '../memory-write.js';
import { forgetMemory as forgetCanonicalMemory } from '../memory-lifecycle.js';
import { queryMemory as getCanonicalMemory } from '../memory-recall.js';
import { queryMemorySemantic as recallCanonicalMemory } from '../memory-semantic.js';
import { insertNotification } from '../notifications-core.js';
import { deletePrunableSignals } from '../message-lifecycle.js';
import { canonicalizePath, repositoryWorkspacePaths } from '../git.js';
import { countInboxMessages, listInboxMessagesPage, type MessageListParams, type MessagePage } from './coordination-message-inbox.js';
import { evaluateVerifiedMemory as evaluateVerifiedMemoryCanonical, recallVerifiedMemory as recallVerifiedMemoryCanonical, storeVerifiedMemory as storeVerifiedMemoryCanonical, type VerifiedMemoryPageV1, type VerifiedMemoryRecallParams, type VerifiedMemoryStoreParams, type VerifiedMemoryV1 } from './verified-memory.js';
import { containsSecretLikeText } from '../memory-hardening.js';
import type { MemoryEvaluationCorpusV1, MemoryEvaluationReportV1 } from '../memory-hardening.js';

export abstract class CoordinationMemoryAgents extends CoordinationState {
  protected get canonicalWorkspace(): string {
    return canonicalizePath(this.workspace);
  }

  storeVerifiedMemory(params: VerifiedMemoryStoreParams): VerifiedMemoryV1 {
    return storeVerifiedMemoryCanonical({ db: this.db, canonicalWorkspace: this.canonicalWorkspace, writeTransaction: operation => this.writeTransaction(operation), embedMemory: (memoryId, text) => this.embedMemory(memoryId, text) }, params);
  }

  recallVerifiedMemory(params: VerifiedMemoryRecallParams = {}): VerifiedMemoryPageV1 {
    return recallVerifiedMemoryCanonical({ db: this.db, canonicalWorkspace: this.canonicalWorkspace, writeTransaction: operation => this.writeTransaction(operation), embedMemory: (memoryId, text) => this.embedMemory(memoryId, text) }, params);
  }

  evaluateVerifiedMemory(params: { corpus?: MemoryEvaluationCorpusV1; now?: string; limit?: number; minSimilarity?: number } = {}): MemoryEvaluationReportV1 {
    return evaluateVerifiedMemoryCanonical((recall) => this.recallVerifiedMemory(recall), params);
  }

  storeMemory(params: { label: string; text: string; tags?: string | string[] | null }): MemoryItem {
    const label = required(params.label, 'label');
    const text = required(params.text, 'text');
    if (containsSecretLikeText(`${label}\n${text}`)) throw new Error('memory rejected: secret-like content must never enter durable memory');
    const guarded = insertPreparedMemoryWithSimilarityGate(this.db, {
      agentId: 'awareness',
      taskContext: label,
      observation: text,
      importance: 5,
      label,
      tags: splitTags(params.tags),
      workspacePath: this.canonicalWorkspace,
    });
    if (guarded.skipped) {
      const similarId = guarded.similar[0]?.memory_id;
      if (!similarId) throw new Error('similar memory gate returned no candidate');
      const existing = this.db.prepare('SELECT * FROM awareness_memories WHERE memory_id = ?').get(similarId) as CanonicalMemoryRow | undefined;
      if (!existing) throw new Error('similar memory gate returned an unknown memory');
      return memoryFromCanonicalRow(existing);
    }
    const memoryId = guarded.result.memoryId;
    // Best-effort: embed on write when a host embedder is configured. Never blocks the store.
    this.embedMemory(memoryId, `${label}\n${text}`);
    return {
      memoryId,
      label: guarded.result.memory.label,
      text: guarded.result.memory.observation,
      tags: guarded.result.memory.tags,
      createdAt: guarded.result.memory.created_at,
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
    for (const row of rows) if (this.embedMemory(row.memory_id, `${row.label}\n${renderMemoryContent(row.observation)}`)) embedded++;
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

  recallMemory(params: { query?: string | null; label?: string | null; limit?: number; semantic?: boolean; minSimilarity?: number } = {}): MemoryRecallPage {
    const limit = Math.min(Math.max(params.limit ?? 10, 1), 50);
    const common = {
      query: params.query?.trim() ?? '',
      ...(params.label?.trim() ? { label: params.label.trim() } : {}),
      limit,
      workspacePath: this.canonicalWorkspace,
    };
    const result = params.semantic
      ? recallCanonicalMemory(this.db, common, true)
      : getCanonicalMemory(this.db, common);
    const warnings: unknown = (result as Record<string, unknown>)['warnings'];
    const rawMemories = (result['memories'] ?? []) as CanonicalMemoryRow[];
    return {
      memories: rawMemories.map(memoryFromCanonicalRow),
      partial: result['partial'] === true,
      partialReasons: Array.isArray(result['partialReasons'])
        ? result['partialReasons'] as MemoryRecallPage['partialReasons']
        : [],
      ...(result['terminalLimit'] && typeof result['terminalLimit'] === 'object'
        ? { terminalLimit: result['terminalLimit'] as MemoryRecallPage['terminalLimit'] }
        : {}),
      ...(Array.isArray(warnings)
        ? { warnings: warnings.filter((warning: unknown): warning is string => typeof warning === 'string') }
        : {}),
    };
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
    const row = this.db.prepare('SELECT s.*, NULL AS read_at FROM signals s WHERE s.workspace_path IN (SELECT value FROM json_each(?)) AND s.signal_id = ?')
      .get(JSON.stringify(repositoryWorkspacePaths(this.canonicalWorkspace)), messageId) as unknown as CanonicalMessageRow | undefined;
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
    const metadataJson = JSON.stringify({
      ...(existing ? parseMetadata(existing.metadata_json) : {}),
      ...parseMetadata(params.metadata),
    });
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
    const clauses: string[] = ['workspace_path IN (SELECT value FROM json_each(?))'];
    const values: string[] = [JSON.stringify(repositoryWorkspacePaths(this.canonicalWorkspace))];
    if (!params.includeLeft || params.staleAfterMs) clauses.push("status != 'LEFT'");
    if (params.staleAfterMs) {
      clauses.push('last_seen_at < ?');
      values.push(cutoffIso(params.staleAfterMs));
    }
    const where = ` WHERE ${clauses.join(' AND ')}`;
    const rows = this.db.prepare(`SELECT * FROM awareness_agents${where} ORDER BY last_seen_at DESC, agent_id ASC`).all(...values);
    return (rows as unknown as CanonicalAgentRow[]).map(agentFromCanonicalRow);
  }

  sendMessage(params: { fromAgentId: string; toAgentId?: string | null; topic?: string | null; text: string; data?: import('../signal-data.js').SignalData | string; files?: string | string[] | null }): LiteMessage {
    const fromAgentId = required(params.fromAgentId, 'from-agent-id');
    const toAgentId = params.toAgentId?.trim() || null;
    const topic = params.topic?.trim() || null;
    const messageText = encodeSignalBody(required(params.text, 'text'), params.data);
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

  countMessages(params: Omit<MessageListParams, 'cursor' | 'limit'> = {}): number {
    return countInboxMessages(this.db, this.canonicalWorkspace, params);
  }

  /** Bounded internal previews; public read surfaces use listMessagesPage. */
  listMessages(params: MessageListParams = {}): LiteMessage[] {
    return this.listMessagesPage(params).messages;
  }

  listMessagesPage(params: MessageListParams = {}): MessagePage {
    return listInboxMessagesPage(this.db, this.canonicalWorkspace, params);
  }

  markMessageRead(params: { messageId: string; agentId: string }): LiteMessage {
    const message = this.getMessage(required(params.messageId, 'message-id'));
    const agentId = required(params.agentId, 'agent-id');
    if (message.fromAgentId === agentId || (message.toAgentId && message.toAgentId !== agentId)) {
      throw new Error(`message ${message.messageId} is not addressed to ${agentId}`);
    }
    this.touchAgent({ agentId });
    const readAt = now();
    this.db.prepare(`INSERT INTO signal_reads(signal_id, agent_id, read_at)
      VALUES (?, ?, ?)
      ON CONFLICT(signal_id, agent_id) DO UPDATE SET read_at = excluded.read_at`).run(message.messageId, agentId, readAt);
    return { ...message, readAt };
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
import { renderMemoryContent } from '../memory-content.js';
