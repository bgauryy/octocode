import type { DatabaseSync } from 'node:sqlite';
import { renewWorkLease } from './work.js';
import type { FileLockParams, FileLockResult } from './types.js';
import { activeLockRows, preFlightIntent } from './intents-preflight.js';
import { releaseFileLock } from './intents-release.js';

export function fileLock(db: DatabaseSync, params: FileLockParams): FileLockResult {
  switch (params.type) {
    case 'lock': {
      const result = preFlightIntent(db, {
        agentId: params.agentId,
        sessionId: params.sessionId,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        runId: params.runId,
        targetFiles: params.targetFiles ?? [],
        ttlMs: params.ttlMs,
        rationale: params.reasoning?.trim() || 'manual: fileLock lock',
        testPlan: 'release or verify file-lock run',
      });
      if (!result.ok) return { ok: false, type: 'lock', conflict: true, conflicts: result.conflicts };
      const locks = activeLockRows(db, { runId: result.run.run_id });
      return {
        ok: true,
        type: 'lock',
        runId: result.run.run_id,
        files: result.run.target_files,
        reasoning: params.reasoning?.trim() || 'manual: fileLock lock',
        acquiredAt: result.run.locks[0]?.acquired_at ?? null,
        expiresAt: result.run.locks[0]?.expires_at ?? null,
        locks,
      };
    }
    case 'release': {
      if (!params.runId && (!params.targetFiles || params.targetFiles.length === 0)) {
        throw new Error('fileLock release requires runId or targetFiles');
      }
      const rel = releaseFileLock(db, {
        agentId: params.agentId,
        sessionId: params.sessionId,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        runId: params.runId,
        targetFiles: params.targetFiles,
        status: params.status,
        verified: params.verified,
        verifiedNote: params.verifiedNote,
      });
      return {
        ok: !('unverifiedConclusion' in rel),
        type: 'release',
        ...rel,
      };
    }
    case 'status':
      return {
        ok: true,
        type: 'status',
        locks: activeLockRows(db, {
          workspacePath: params.workspacePath,
          artifact: params.artifact,
          agentId: params.agentId,
          sessionId: params.sessionId,
          runId: params.runId,
        }),
      };
    case 'renew': {
      if (!params.runId) throw new Error('fileLock renew requires runId');
      const agentId = params.agentId ?? 'agent';
      const renewed = renewWorkLease(db, {
        agentId, runId: params.runId, ttlMs: params.ttlMs,
      }, { exclusiveOnly: true });
      return {
        ok: true,
        type: 'renew',
        runId: params.runId,
        renewed: renewed.locksRenewed > 0,
        locks_renewed: renewed.locksRenewed,
        expiresAt: renewed.expiresAt,
      };
    }
  }
}
