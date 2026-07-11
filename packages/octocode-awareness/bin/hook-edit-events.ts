import { relative } from 'node:path';
import { insertEditLog } from '../src/audit.js';
import { activeTaskClaimForAgent } from '../src/tasks.js';
import { endWork, startWork, touchWork } from '../src/work.js';
import { discardUncommittedHookFiles } from '../src/work-hook.js';
import { evaluateHarnessGuard } from '../src/pi-hooks.js';
import { agentId, artifact, completeHookControl, db, emitHookContext, extractFiles, hookBlockOutcome, hookSkillRoot, hookToolFailed, resolveHookPath, shellHookHost, workspace } from './hook-payload.js';
import { emitPeerDelta, registerHookAgent } from './hook-peers.js';
import { activeRunForFiles, consumeHookRun, isAggregatedFallbackHookRun, recordHookRun, refreshFallbackVerificationPlan, runOrigin, startOrAttachFallbackHookRun, withHookDbRetry } from './hook-run-state.js';

export async function runPreEdit(payload: Record<string, unknown>): Promise<number> {
  const files = extractFiles(payload);
  if (files.length === 0) return 0;
  const hookWorkspace = workspace(payload) ?? process.cwd();
  const guardReason = evaluateHarnessGuard({
    targetFiles: files,
    skillRoot: hookSkillRoot(payload),
    cwd: hookWorkspace,
  });
  if (guardReason) {
    return completeHookControl(hookBlockOutcome(
      shellHookHost(payload),
      'pre-edit',
      `${guardReason} Edit blocked.`,
    ));
  }
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:pre-edit');
    const hookAgentId = agentId(payload);
    const hookArtifact = artifact(payload);
    const activeClaim = activeTaskClaimForAgent(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
    });
    const explicitRunId = activeClaim ? null : activeRunForFiles(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
      files,
      origins: ['WORK'],
    });
    const result = explicitRunId
      ? { ok: true as const, ...touchWork(database, {
        agentId: hookAgentId,
        runId: explicitRunId,
        targetFiles: files,
        ttlMs: 10 * 60_000,
      }) }
      : activeClaim
        ? startWork(database, {
          agentId: hookAgentId,
          workspacePath: hookWorkspace,
          artifact: hookArtifact,
          runId: activeClaim.run_id,
          targetFiles: files,
          origin: 'HOOK',
          source: 'HOOK',
          ttlMs: 10 * 60_000,
        })
        : startOrAttachFallbackHookRun(database, payload, hookWorkspace, files);
    if (!result.ok) {
      const detail = result.conflicts.slice(0, 3)
        .map(conflict => `${relative(hookWorkspace, conflict.file_path)} (${conflict.agent_id})`)
        .join(', ');
      return completeHookControl(hookBlockOutcome(
        shellHookHost(payload),
        'pre-edit',
        `octocode-awareness: exclusive file work blocks this edit${detail ? `: ${detail}` : ''}.`,
      ));
    }
    withHookDbRetry(() => refreshFallbackVerificationPlan(database, result.run.run_id, hookWorkspace));
    recordHookRun(payload, files, hookWorkspace, result.run.run_id);
    const peerContext = emitPeerDelta(payload, files, hookWorkspace, result.peers);
    if (peerContext) {
      emitHookContext(
        payload,
        shellHookHost(payload) === 'cursor' ? 'preToolUse' : 'PreToolUse',
        peerContext,
      );
    }
    return 0;
  } catch (error) {
    console.error(`octocode-awareness pre-flight warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
    return 0;
  }
}

export async function runPostEdit(payload: Record<string, unknown>): Promise<number> {
  const files = extractFiles(payload);
  if (files.length === 0) return 0;
  const hookWorkspace = workspace(payload) ?? process.cwd();
  let consumedRunId: string | null = null;
  let stage = 'open database';
  try {
    const database = db();
    stage = 'register hook agent';
    withHookDbRetry(() => registerHookAgent(database, payload, 'hook:post-edit'));
    const hookAgentId = agentId(payload);
    const hookArtifact = artifact(payload);
    stage = 'consume correlation';
    consumedRunId = withHookDbRetry(() => consumeHookRun(database, payload, files, hookWorkspace));
    stage = 'resolve fallback run';
    const correlatedRunId = consumedRunId
      ?? withHookDbRetry(() => activeTaskClaimForAgent(database, {
        agentId: hookAgentId,
        workspacePath: hookWorkspace,
        artifact: hookArtifact,
      }))?.run_id
      ?? withHookDbRetry(() => activeRunForFiles(database, {
        agentId: hookAgentId,
        workspacePath: hookWorkspace,
        artifact: hookArtifact,
        files,
        origins: ['WORK', 'HOOK'],
      }));
    if (!correlatedRunId) {
      console.error('octocode-awareness post-edit warning (continuing): could not identify a unique work run; leaving presence for expiry.');
      return 0;
    }
    stage = 'read run origin';
    const origin = withHookDbRetry(() => runOrigin(database, correlatedRunId));
    if (hookToolFailed(payload)) {
      if (origin === 'HOOK') {
        const discarded = withHookDbRetry(() => discardUncommittedHookFiles(database, {
          agentId: hookAgentId,
          runId: correlatedRunId,
          targetFiles: files,
          workspacePath: hookWorkspace,
        }));
        if (!discarded.deletedRun) {
          withHookDbRetry(() => refreshFallbackVerificationPlan(database, correlatedRunId, hookWorkspace));
        }
      }
      consumedRunId = null;
      return 0;
    }
    stage = 'finish work lifecycle';
    if (origin === 'HOOK' && isAggregatedFallbackHookRun(database, correlatedRunId)) {
      withHookDbRetry(() => touchWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        ttlMs: 10 * 60_000,
      }));
    } else if (origin === 'HOOK') {
      withHookDbRetry(() => endWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        targetFiles: files,
      }));
    } else {
      withHookDbRetry(() => touchWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        targetFiles: files,
        ttlMs: 10 * 60_000,
      }));
    }
    // The lifecycle mutation committed, so this correlation must not be
    // restored even if a later audit-log write fails.
    consumedRunId = null;
    stage = 'write edit log';
    for (const file of files) {
      withHookDbRetry(() => insertEditLog(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        filePath: resolveHookPath(file, hookWorkspace),
        operation: 'update',
        workspacePath: hookWorkspace,
        artifact: hookArtifact,
      }));
    }
  } catch (error) {
    // Consuming the file-backed correlation and mutating SQLite cannot be one
    // atomic transaction. Restore it on a failed lifecycle mutation so a host
    // retry can finish the same run instead of leaving presence orphaned.
    if (consumedRunId) {
      try { recordHookRun(payload, files, hookWorkspace, consumedRunId); } catch { /* best effort */ }
    }
    console.error(`octocode-awareness post-edit warning during ${stage} (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return 0;
}
