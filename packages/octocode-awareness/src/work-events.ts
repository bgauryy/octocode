import type { DatabaseSync } from 'node:sqlite';
import { appendDomainEvent } from './event-outbox.js';

export function appendWorkEvent(db: DatabaseSync, input: {
  workspace: string;
  type: 'work.started' | 'work.extended' | 'work.touched' | 'work.ended' | 'work.protection-released';
  agentId: string;
  runId: string;
  sessionId?: string | null;
  createdAt: string;
  payload: Record<string, unknown>;
}): void {
  appendDomainEvent(db, {
    workspace: input.workspace,
    eventType: input.type,
    retentionClass: 'operational',
    actorId: input.agentId,
    aggregateKind: 'run',
    aggregateId: input.runId,
    aggregateRevision: input.createdAt,
    sessionId: input.sessionId,
    createdAt: input.createdAt,
    payload: input.payload,
    eventIdPrefix: 'wevt',
  });
}
