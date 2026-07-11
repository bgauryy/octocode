import type { DatabaseSync } from 'node:sqlite';
import { fileLock } from './intents.js';
import { agentSignal } from './notifications.js';
import type { AgentSignalResult, FileLockResult, NotificationKind, RunStatus } from './types.js';
import { stringArray } from './tool-operations-shared.js';
import type { AwarenessToolOperation, AwarenessToolOperationContext, AwarenessToolOperationResult } from './tool-operations.js';

export function runSignalsOperation(
  db: DatabaseSync,
  operation: AwarenessToolOperation,
  request: Record<string, unknown>,
  context: AwarenessToolOperationContext,
): AwarenessToolOperationResult | null {
  const cwd = context.cwd ?? process.cwd();
  const agentId = context.agentId ?? 'agent';
  switch (operation) {
case 'agent_signal': {
      const rawAction = request['action'];
      if (rawAction !== 'publish' && rawAction !== 'list' && rawAction !== 'reply' && rawAction !== 'resolve' && rawAction !== 'ack') {
        throw new Error('agent_signal requires action: publish | list | reply | resolve | ack');
      }
      const toAgents = Array.isArray(request['to_agents']) ? request['to_agents'] as string[] : request['to_agent'] ? [String(request['to_agent'])] : [];
      const refs = Array.isArray(request['refs']) ? request['refs'] as string[] : [];
      const kinds = Array.isArray(request['kinds']) ? request['kinds'] as NotificationKind[] : [];
      const result = agentSignal(db, {
        action: rawAction,
        agentId: (request['agent_id'] as string | undefined) ?? agentId,
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        kind: request['kind'] as NotificationKind | undefined,
        subject: request['subject'] as string | undefined,
        body: request['body'] as string | undefined ?? null,
        toAgents,
        files: stringArray(request['files']),
        refs,
        importance: request['importance'] as number | undefined,
        inReplyTo: (request['in_reply_to'] as string | undefined) ?? null,
        threadId: (request['thread_id'] as string | undefined) ?? null,
        signalIds: stringArray(request['signal_ids']),
        unreadOnly: request['unread_only'] as boolean | undefined,
        markRead: request['mark_read'] as boolean | undefined,
        kinds,
        limit: request['limit'] as number | undefined,
        cwd,
      }) as AgentSignalResult;
      return { payload: result, exitCode: 0 };
    }
case 'file_lock': {
      const rawType = request['type'];
      if (rawType !== 'lock' && rawType !== 'release' && rawType !== 'status' && rawType !== 'renew') {
        throw new Error('file_lock requires type: lock | release | status | renew');
      }
      const lockAgentId = (request['agentId'] as string | undefined) ?? (request['agent_id'] as string | undefined) ?? agentId;
      const workspacePath = (request['workspace_path'] as string | undefined) ?? cwd;
      const targetFiles = (request['targetFiles'] as string[] | undefined) ?? (request['target_files'] as string[] | undefined) ?? [];
      const result = fileLock(db, {
        type: rawType,
        agentId: lockAgentId,
        sessionId: (request['sessionId'] as string | undefined) ?? (request['session_id'] as string | undefined) ?? context.sessionId ?? null,
        workspacePath,
        runId: (request['runId'] as string | undefined) ?? (request['run_id'] as string | undefined) ?? null,
        targetFiles,
        ttlMs: (request['ttlMs'] as number | undefined) ?? (request['ttl_ms'] as number | undefined) ?? null,
        status: request['status'] as RunStatus | undefined,
        verified: request['verified'] as boolean | undefined,
        verifiedNote: (request['verifiedNote'] as string | undefined) ?? (request['verified_note'] as string | undefined),
        reasoning: request['reasoning'] as string | undefined,
      }) as FileLockResult;
      if (result.type === 'lock' && result.ok === false && request['signal_on_conflict'] !== false) {
        const conflictAgents = [...new Set(result.conflicts.map((conflict) => conflict.agent_id))];
        agentSignal(db, {
          action: 'publish',
          agentId: lockAgentId,
          workspacePath,
          toAgents: conflictAgents,
          kind: 'blocker',
          subject: `File lock conflict: ${targetFiles.slice(0, 3).join(', ') || 'target file'}`,
          body: JSON.stringify({ conflicts: result.conflicts }),
          files: targetFiles,
          importance: 7,
          cwd,
        });
      }
      return { payload: result, exitCode: result.ok === false ? 2 : 0 };
    }
  }
  return null;
}
