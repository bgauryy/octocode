import { normalizeWorkspacePath } from '../git.js';
import type { AgentRecord,AgentStatus,CheckAudit,CheckStatus,HandoffNote,LiteMessage,Lock,LockWaitResult,MemoryItem,Plan,PlanGraphResult,PlanStatus,PruneResult,SourceStep,Task,TaskStatus,WorkPresence } from '@octocodeai/octocode-shared/entities';
import type { DatabaseSync } from '@octocodeai/octocode-shared/sqlite';
import { resolve } from 'node:path';
import { defaultDbPath,type AwarenessOptions,type AwarenessSchema } from './coordination-shared.js';
import { type AgentEventEnvelopeV1 } from '../continuity-contracts.js';
import { connectDb, resolveDbPath } from '../db-runtime.js';
import { beginWrite } from '../db-transaction.js';
import { insertOutboxEvent } from '../event-outbox.js';
import type { MemoryEvaluationCorpusV1,MemoryEvaluationReportV1,MemoryRecallModeV1 } from '../memory-hardening.js';
import type { VerifiedMemoryV1 } from './coordination-memory-agents.js';

export abstract class CoordinationBase {
  readonly workspace: string;
  readonly dbPath: string;
  protected readonly db: DatabaseSync;

  constructor(options: AwarenessOptions) {
    const workspace = resolve(options.workspace ?? process.cwd());
    this.workspace = normalizeWorkspacePath(workspace, workspace) ?? workspace;
    this.dbPath = resolveDbPath(options.dbPath ?? defaultDbPath(this.workspace, options.scope));
    this.db = connectDb(this.dbPath);
  }

  close(): void {
    this.db.close();
  }

  protected writeTransaction<T>(operation: () => T): T {
    const transaction = beginWrite(this.db);
    try {
      const result = operation();
      transaction.commit();
      return result;
    } catch (error) {
      transaction.rollback();
      throw error;
    }
  }

  protected insertOutboxEvent(input: AgentEventEnvelopeV1): number {
    return insertOutboxEvent(this.db, input);
  }

  abstract createPlan(params: { agentId: string; title: string; goal?: string | null }): Plan;
  abstract listPlans(status?: PlanStatus): Plan[];
  abstract getPlan(planId: string): Plan;
  abstract donePlan(params: { planId: string; agentId: string }): Plan;
  abstract addTask(params: {
    agentId: string;
    planId: string;
    title: string;
    filePath?: string | null;
    paths?: string | string[] | null;
    reasoning?: string | null;
    acceptance?: string | null;
    checkCommand?: string | null;
    dependsOn?: string | string[] | null;
    priority?: number;
  }): Task;
  abstract addTaskDependency(params: { taskId: string; dependsOnTaskId: string; agentId: string }): Task;
  abstract listTasks(params: { planId?: string; status?: TaskStatus; agentId?: string }): Task[];
  abstract listReadyTasks(params: { planId?: string; limit?: number }): Task[];
  abstract getTask(taskId: string): Task;
  abstract claimTask(params: { taskId: string; agentId: string; leaseSeconds?: number }): Task;
  abstract heartbeatTask(params: { taskId: string; runId: string; agentId: string; leaseSeconds?: number }): Task;
  abstract releaseTask(params: { taskId: string; runId: string; agentId: string; blockedReason?: string | null }): Task;
  abstract doneTask(params: { taskId: string; runId: string; agentId: string }): Task;
  abstract acquireLock(params: { filePath: string; runId?: string; testPlan?: string; agentId: string; reason?: string | null; ttlSeconds?: number }): Lock;
  abstract listLocks(): Lock[];
  abstract waitForLock(params: { filePath: string; agentId?: string | null; waitMs?: number; retryIntervalMs?: number }): LockWaitResult;
  abstract pruneLocks(params: { dryRun?: boolean }): { dryRun: boolean; matched: number; deleted: number };
  abstract releaseLock(params: { filePath: string; runId: string; agentId: string }): { released: boolean };
  abstract startWork(params: { filePath: string; runId?: string; testPlan?: string; agentId: string; reason?: string | null; ttlSeconds?: number }): WorkPresence;
  abstract listWork(params: { filePath?: string | null; agentId?: string | null }): WorkPresence[];
  abstract showWork(params: { filePath: string }): WorkPresence[];
  abstract endWork(params: { filePath: string; runId: string; agentId: string }): { ended: boolean };
  abstract addHandoff(params: { agentId: string; summary: string; files?: string | string[] | null }): HandoffNote;
  abstract listHandoffs(params: { includeCleared?: boolean }): HandoffNote[];
  abstract clearHandoff(params: { handoffId: string }): { cleared: boolean };
  abstract auditChecks(params: { agentId?: string | null; planId?: string | null; minAgeMs?: number | null }): CheckAudit;
  abstract markCheck(params: { taskId: string; runId: string; doneAt: string; agentId: string; message: string; status?: CheckStatus }): Task;
  abstract storeMemory(params: { label: string; text: string; tags?: string | string[] | null }): MemoryItem;
  abstract storeVerifiedMemory(params: { label: string; text: string; scope?: 'project' | 'artifact'; sourceDigest: string; verifiedAt?: string; validUntil?: string; importance?: number; tags?: string | string[] | null }): VerifiedMemoryV1;
  abstract recallVerifiedMemory(params?: { query?: string; label?: string; sourceDigest?: string; scope?: 'project' | 'artifact'; limit?: number; now?: string; mode?: MemoryRecallModeV1; minSimilarity?: number }): VerifiedMemoryV1[];
  abstract evaluateVerifiedMemory(params?: { corpus?: MemoryEvaluationCorpusV1; now?: string; limit?: number; minSimilarity?: number }): MemoryEvaluationReportV1;
  protected abstract embedMemory(memoryId: string, text: string): boolean;
  abstract reindexMemories(params: { force?: boolean; limit?: number }): { enabled: boolean; scanned: number; embedded: number };
  abstract forgetMemory(params: { memoryId: string }): { forgotten: boolean };
  abstract recallMemory(params: { query?: string | null; label?: string | null; limit?: number; semantic?: boolean; minSimilarity?: number }): MemoryItem[];
  abstract pruneMemories(params: { olderThanMs: number; label?: string | null; dryRun?: boolean }): PruneResult;
  abstract joinAgent(params: { agentId: string; name?: string | null; role?: string | null; metadata?: string | Record<string, unknown> | null }): AgentRecord;
  abstract touchAgent(params: { agentId: string; status?: AgentStatus }): AgentRecord;
  abstract leaveAgent(params: { agentId: string }): AgentRecord;
  abstract listAgents(params: { includeLeft?: boolean; staleAfterMs?: number }): AgentRecord[];
  abstract sendMessage(params: { fromAgentId: string; toAgentId?: string | null; topic?: string | null; text: string; files?: string | string[] | null }): LiteMessage;
  abstract listMessages(params: { agentId?: string | null; includeRead?: boolean; topic?: string | null; limit?: number }): LiteMessage[];
  abstract markMessageRead(params: { messageId: string; agentId: string }): LiteMessage;
  abstract pruneMessages(params: { olderThanMs: number; readOnly?: boolean; dryRun?: boolean }): PruneResult;
  abstract schema(): AwarenessSchema;
  abstract schemaCommand(command?: string): unknown;
  protected abstract getMemory(memoryId: string): MemoryItem;
  protected abstract getAgent(agentId: string): AgentRecord;
  protected abstract getMessage(messageId: string): LiteMessage;
  protected abstract getHandoff(handoffId: string): HandoffNote;
  protected abstract countMemories(): number;
  protected abstract countSignals(): number;
  protected abstract countOpenHandoffs(): number;
  protected abstract countStaleAgents(staleAfterMs: number): number;
  protected abstract countPresentAgents(staleAfterMs: number): number;
  abstract getPlanBySourceKey(params: { sourceKind: string; sourceKey: string }): Plan | null;
  abstract reconcilePlanGraph(params: { planId: string }): Map<string, Task>;
  abstract abandonPlan(params: { planId: string; agentId: string; reason?: string | null }): { plan: Plan; cancelled: number };
  abstract materializePlanGraph(params: {
    agentId: string;
    sourcePlanKey: string;
    sourceKind?: string | null;
    title: string;
    goal?: string | null;
    rfcPath?: string | null;
    rfcRevision?: string | null;
    steps: SourceStep[];
  }): PlanGraphResult;
}
