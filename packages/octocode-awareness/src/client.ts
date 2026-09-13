import {
  getAwarenessOperationDescriptor,
  listAwarenessOperationDescriptors,
  type AwarenessOperation,
  type AwarenessOperationDescriptor,
  type AwarenessOperationParams,
} from './schema/operation-catalog.js';
import { appendDomainEvent, listOutboxEvents, type DomainEventInput, type OutboxEventPage } from './event-outbox.js';
import { connectDb, resolveDbPath } from './db-runtime.js';
import type { AwarenessInsightCandidate, AwarenessInsightProvider, AwarenessOperationResult } from './operation-contracts.js';
import { storageScopeForOperation } from './workspace-policy.js';
import type { Regulation } from './attend-physiology.js';
import type { assessContextRegulation, ContextObservation, ContextFeedback, ContextAdvisory } from './context-regulation.js';
import type { RunState, ContextNudge } from './context-state.js';

export interface AwarenessClientContext {
  database?: string;
  workspace: string;
  agentId: string;
  sessionId?: string;
  scope?: import('./storage-scope.js').AwarenessStorageScope;
  signal?: AbortSignal;
  insightProvider?: AwarenessInsightProvider;
}

export interface AwarenessHostEventInput extends Omit<DomainEventInput, 'workspace' | 'actorId' | 'createdAt'> {
  createdAt?: string;
}

export interface AwarenessEventCursor {
  afterSequence?: number;
  limit?: number;
  eventType?: string;
  retentionClass?: DomainEventInput['retentionClass'];
}

export interface AwarenessExecutableCall<K extends AwarenessOperation = AwarenessOperation> {
  operation: K;
  params?: AwarenessOperationParams[K];
}

export interface AwarenessPeerSummary {
  actorId: string;
  name?: string;
  status?: string;
  lastSeenAt?: string;
}

export interface AwarenessItemSummary {
  id?: string;
  title?: string;
  detail?: string;
  actorId?: string;
  path?: string;
  status?: string;
  locked?: boolean;
}

export interface AwarenessOrientation {
  run_state: RunState;
  revision: string;
  unchanged: false;
  self: { actorId: string; sessionId?: string };
  peers: { items: AwarenessPeerSummary[]; partial: boolean };
  work: { owned?: AwarenessItemSummary; overlaps: AwarenessItemSummary[] };
  inbox: AwarenessItemSummary[];
  verification: { pending: number; stale: number };
  continuation?: AwarenessItemSummary;
  recovery?: { degraded: boolean; pressure?: string };
  operational: { unavailable: readonly string[]; runtime?: Omit<ReturnType<typeof assessContextRegulation>, 'advisories' | 'run_state'> };
  regulation: Regulation & { next?: AwarenessExecutableCall; advisories?: ContextAdvisory[]; nudge?: ContextNudge };
  next: AwarenessExecutableCall[];
  partial: boolean;
  partialReasons: string[];
  insights?: { advisory: true; candidates: AwarenessInsightCandidate[] };
  knowledge?: { advisory: true } & Awaited<ReturnType<typeof import('./knowledge-memory.js').getKnowledgeBriefing>>;
}

export interface AwarenessOrientationUnchanged {
  revision: string;
  unchanged: true;
}

export type AwarenessOrientationResult = AwarenessOrientation | AwarenessOrientationUnchanged;

export interface AwarenessClient {
  readonly context: Readonly<AwarenessClientContext>;
  orient(params?: AwarenessOperationParams['context.orient']): Promise<AwarenessOrientationResult>;
  observe(params: ContextObservation): Promise<AwarenessOperationResult>;
  feedback(params: ContextFeedback): Promise<AwarenessOperationResult>;
  execute<K extends AwarenessOperation>(call: AwarenessExecutableCall<K>): Promise<AwarenessOperationResult>;
  operations(): readonly AwarenessOperationDescriptor[];
  recordHostEvent(input: AwarenessHostEventInput): Promise<{ sequence: number }>;
  consumeEvents(params?: AwarenessEventCursor): Promise<OutboxEventPage>;
}

function record(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : undefined;
}

function findExecutableCall(value: unknown): AwarenessExecutableCall | undefined {
  if (Array.isArray(value)) {
    for (const child of value) {
      const found = findExecutableCall(child);
      if (found) return found;
    }
    return undefined;
  }
  const object = record(value);
  if (!object) return undefined;
  if (typeof object['operation'] === 'string' && getAwarenessOperationDescriptor(object['operation'])) {
    return {
      operation: object['operation'] as AwarenessOperation,
      ...(record(object['params']) ? { params: object['params'] as AwarenessOperationParams[AwarenessOperation] } : {}),
    };
  }
  for (const child of Object.values(object)) {
    const found = findExecutableCall(child);
    if (found) return found;
  }
  return undefined;
}

function schemaSupportsLimit(schema: Readonly<Record<string, unknown>>): boolean {
  if (record(schema['properties'])?.['limit']) return true;
  return ['oneOf', 'anyOf', 'allOf'].some(key =>
    Array.isArray(schema[key]) && (schema[key] as unknown[]).some(child => Boolean(record(child)) && schemaSupportsLimit(record(child)!)));
}

export function createAwarenessClient(
  context: AwarenessClientContext,
): AwarenessClient {
  const bound = Object.freeze({ ...context });
  const execute = async <K extends AwarenessOperation>(call: AwarenessExecutableCall<K>): Promise<AwarenessOperationResult> => {
    const descriptor = getAwarenessOperationDescriptor(call.operation) as AwarenessOperationDescriptor<K> | undefined;
    if (!descriptor) {
      return { exitCode: 1, payload: { ok: false, operation: call.operation, error: `Unknown Awareness operation: ${call.operation}` } };
    }
    try {
      const params = descriptor.validate(call.params);
      const executed = await descriptor.handler(bound, params);
      const payload = descriptor.continuations(executed.payload);
      const actualBytes = Buffer.byteLength(JSON.stringify(payload));
      if (actualBytes > descriptor.outputBudget) {
        const inputParams = record(params) ?? {};
        const currentLimit = typeof inputParams['limit'] === 'number' ? inputParams['limit'] : undefined;
        const retry = schemaSupportsLimit(descriptor.inputSchema) && (currentLimit === undefined || currentLimit > 1)
          ? {
              operation: call.operation,
              params: {
                ...inputParams,
                ...(call.operation === 'context.orient' ? { if_revision: undefined } : {}),
                // A proportional estimate is unsafe for heterogeneous rows: a
                // retry can still exceed the budget and strand the caller.
                // One row is the only monotonic bounded retry; list handlers
                // then expose their stable cursor/offset for lossless paging.
                limit: 1,
              },
            } as AwarenessExecutableCall
          : findExecutableCall(payload);
        if (!retry) throw new Error(`${call.operation} exceeded its output budget without an executable continuation`);
        return {
          exitCode: 2,
          payload: {
            ok: false,
            error_code: 'OUTPUT_BUDGET_EXCEEDED',
            operation: call.operation,
            completed: executed.exitCode === 0,
            effect: descriptor.effect(params),
            budget_bytes: descriptor.outputBudget,
            actual_bytes: actualBytes,
            next: { retry },
          },
        };
      }
      return { ...executed, payload };
    } catch (error) {
      const issues = error && typeof error === 'object' && Array.isArray((error as { issues?: unknown }).issues)
        ? (error as { issues: unknown[] }).issues : undefined;
      return {
        exitCode: 1,
        payload: {
          ok: false,
          operation: call.operation,
          error: error instanceof Error ? error.message : String(error),
          ...(issues ? { issues } : {}),
        },
      };
    }
  };
  return Object.freeze({
    context: bound,
    observe: (params: ContextObservation) => execute({ operation: 'context.observe', params }),
    feedback: (params: ContextFeedback) => execute({ operation: 'context.feedback', params }),
    operations: listAwarenessOperationDescriptors,
    async recordHostEvent(input: AwarenessHostEventInput): Promise<{ sequence: number }> {
      const scope = storageScopeForOperation('host.events.record', bound.workspace, bound.scope);
      const db = connectDb(resolveDbPath(bound.database, { scope, workspace: bound.workspace }));
      db.exec('BEGIN IMMEDIATE');
      try {
        const sequence = appendDomainEvent(db, {
          ...input,
          workspace: bound.workspace,
          actorId: bound.agentId,
          createdAt: input.createdAt ?? new Date().toISOString(),
          ...(bound.sessionId && input.sessionId === undefined ? { sessionId: bound.sessionId } : {}),
        });
        db.exec('COMMIT');
        return { sequence };
      } catch (error) {
        try { db.exec('ROLLBACK'); } catch { /* transaction already ended */ }
        throw error;
      } finally { db.close(); }
    },
    async consumeEvents(params: AwarenessEventCursor = {}): Promise<OutboxEventPage> {
      const scope = storageScopeForOperation('host.events.consume', bound.workspace, bound.scope);
      const db = connectDb(resolveDbPath(bound.database, { scope, workspace: bound.workspace }));
      try { return listOutboxEvents(db, { workspace: bound.workspace, ...params }); }
      finally { db.close(); }
    },
    async orient(params?: AwarenessOperationParams['context.orient']): Promise<AwarenessOrientationResult> {
      const result = await execute({ operation: 'context.orient', ...(params ? { params } : {}) });
      if (result.exitCode !== 0) throw new Error(`context.orient failed: ${JSON.stringify(result.payload)}`);
      return result.payload as AwarenessOrientationResult;
    },
    execute,
  });
}
