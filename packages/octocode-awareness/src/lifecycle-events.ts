import type { DatabaseSync } from 'node:sqlite';
import { appendDomainEvent } from './event-outbox.js';
import { normalizeWorkspacePath } from './git.js';

export function appendLifecycleEvent(db: DatabaseSync, input: {
  workspace: string | null;
  type: 'agent.registered' | 'agent.touched' | 'session.started' | 'session.ended';
  agentId: string;
  aggregateKind: 'agent' | 'session';
  aggregateId: string;
  sessionId?: string | null;
  createdAt: string;
  payload?: Record<string, unknown>;
}): void {
  appendDomainEvent(db, {
    workspace: input.workspace || normalizeWorkspacePath(process.cwd(), process.cwd()) || process.cwd(),
    eventType: input.type,
    retentionClass: 'operational',
    actorId: input.agentId,
    aggregateKind: input.aggregateKind,
    aggregateId: input.aggregateId,
    aggregateRevision: input.createdAt,
    sessionId: input.sessionId,
    createdAt: input.createdAt,
    payload: input.payload ?? {},
    eventIdPrefix: 'levt',
  });
}
