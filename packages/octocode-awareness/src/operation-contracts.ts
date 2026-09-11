import type { ApprovalClass } from '@octocodeai/agent-contracts/protocols';
import type { AwarenessStorageScope } from './storage-scope.js';

export type AwarenessOperationEffect =
  | 'read'
  | 'coordination-write'
  | 'workspace-write'
  | 'host-config-write'
  | 'destructive-admin';

export interface CanonicalOperationResult {
  payload: unknown;
  exitCode: number;
  text?: string;
  diagnostics?: string[];
  cancelled?: boolean;
}

export type CanonicalDomainHandler =
  | 'plan' | 'task' | 'work' | 'query'
  | 'verify' | 'verify-audit' | 'signal'
  | 'memory-record' | 'memory-recall' | 'history'
  | 'lock-acquire' | 'lock-wait' | 'lock-release';

export interface CanonicalRouteBinding {
  command: string;
  schema: Readonly<Record<string, unknown>>;
  handler: CanonicalDomainHandler;
  action?: string;
  effect: AwarenessOperationEffect;
  approval?: ApprovalClass;
}

export interface CanonicalExecutionContext {
  database?: string;
  workspace: string;
  agentId: string;
  sessionId?: string;
  scope?: AwarenessStorageScope;
  signal?: AbortSignal;
  insightProvider?: AwarenessInsightProvider;
}

export interface AwarenessInsightCandidate {
  summary: string;
  attribution: string;
  confidence: number;
  path?: string;
}

export interface AwarenessInsightProvider {
  suggest(input: Readonly<{
    workspace: string;
    agentId: string;
    overlaps: readonly Record<string, unknown>[];
    limit: number;
  }>): Promise<readonly AwarenessInsightCandidate[]>;
}
