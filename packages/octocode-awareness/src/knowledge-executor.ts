import { resolve } from 'node:path';
import { connectDb, resolveDbPath } from './db-runtime.js';
import { normalizeWorkspacePath } from './git.js';
import { storageScopeForOperation } from './workspace-policy.js';
import type { AwarenessOperationResult, CanonicalExecutionContext } from './operation-contracts.js';
import { executeExperience } from './experience.js';
import { executeKnowledgeMemory } from './knowledge-memory.js';

export type KnowledgeOperation = 'memory.set' | 'memory.get' | 'memory.revalidate' | 'history.experience';

/** Shared CLI/API binding boundary. Domain owners control their own transactions. */
export async function executeKnowledgeOperation(
  operation: KnowledgeOperation,
  context: CanonicalExecutionContext,
  input: unknown,
): Promise<AwarenessOperationResult> {
  context.signal?.throwIfAborted();
  const workspace = normalizeWorkspacePath(context.workspace, context.workspace) ?? resolve(context.workspace);
  const scope = storageScopeForOperation(operation, workspace, context.scope);
  const db = connectDb(resolveDbPath(context.database, { scope, workspace }));
  const binding = { workspace, actorId: context.agentId, sessionId: context.sessionId };
  try {
    const payload = operation === 'history.experience'
      ? await executeExperience(db, binding, input)
      : await executeKnowledgeMemory(db, binding, operation, input);
    return { exitCode: payload.ok === false ? 1 : 0, payload };
  } catch (error) {
    if (error instanceof Error && 'code' in error && typeof error.code === 'string' && 'terminal_limit' in error) {
      return { exitCode: 2, payload: { ok: false, partial: true, error_code: error.code,
        error: error.message, terminal_limit: error.terminal_limit } };
    }
    throw error;
  } finally { db.close(); }
}
