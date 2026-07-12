import type { DatabaseSync } from 'node:sqlite';
import { runMemoryOperation } from './tool-operations-memory.js';
import { runLearningOperation } from './tool-operations-learning.js';
import { runWorkspaceOperation } from './tool-operations-workspace.js';
import { runVerificationOperation } from './tool-operations-verification.js';
import { runRepositoryOperation } from './tool-operations-repository.js';
import { runSignalsOperation } from './tool-operations-signals.js';
export type AwarenessToolOperation =
  | 'recall'
  | 'record'
  | 'reflect'
  | 'workspace_status'
  | 'refine_get'
  | 'verify_audit'
  | 'verify'
  | 'digest'
  | 'forget'
  | 'agent_signal'
  | 'file_lock'
  | 'mine_weakness'
  | 'export_harness'
  | 'attend'
  | 'query'
  | 'view'
  | 'wiki_sync';
export interface AwarenessToolOperationContext {
  agentId?: string | null;
  cwd?: string | null;
  sessionId?: string | null;
}
export interface AwarenessToolOperationResult {
  payload: unknown;
  exitCode: number;
}

export function runAwarenessToolOperation(
  db: DatabaseSync,
  operation: AwarenessToolOperation,
  request: Record<string, unknown>,
  context: AwarenessToolOperationContext = {},
): AwarenessToolOperationResult {
  const result = [
    runMemoryOperation(db, operation, request, context),
    runLearningOperation(db, operation, request, context),
    runWorkspaceOperation(db, operation, request, context),
    runVerificationOperation(db, operation, request, context),
    runRepositoryOperation(db, operation, request, context),
    runSignalsOperation(db, operation, request, context),
  ].find(candidate => candidate !== null);
  if (!result) throw new Error(`unsupported awareness operation: ${operation}`);
  return result;
}
