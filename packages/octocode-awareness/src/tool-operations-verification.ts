import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { digest, exportMemoryDoc } from './maintenance.js';
import { auditUnverified, markVerified, type MarkVerifiedResult } from './verify.js';
import type { AwarenessToolOperation, AwarenessToolOperationContext, AwarenessToolOperationResult } from './tool-operations.js';

export function runVerificationOperation(
  db: DatabaseSync,
  operation: AwarenessToolOperation,
  request: Record<string, unknown>,
  context: AwarenessToolOperationContext,
): AwarenessToolOperationResult | null {
  const cwd = context.cwd ?? process.cwd();
  const agentId = context.agentId ?? 'agent';
  switch (operation) {
case 'verify': {
      const singleId = request['run_id'] as string | undefined;
      const batchIds = Array.isArray(request['run_ids']) ? (request['run_ids'] as unknown[]).map(String) : [];
      const allPending = Boolean(request['all_pending'] ?? request['allPending']);
      const verifyStatus = ((request['status'] as string | undefined) ?? 'SUCCESS') as 'SUCCESS' | 'FAILED';
      const verifyWorkspace = (request['workspace'] as string | undefined)
        ?? (request['workspace_path'] as string | undefined)
        ?? cwd;
      const verifyArtifact = request['artifact'] as string | undefined;
      const verifyMessage = request['message'] as string | undefined;

      if (allPending && !singleId && batchIds.length === 0) {
        const r = markVerified(db, {
          allPending: true,
          agentId,
          workspacePath: verifyWorkspace,
          artifact: verifyArtifact,
          message: verifyMessage,
          status: verifyStatus,
        }) as MarkVerifiedResult;
        if (!r.ok) {
          return { payload: { run_id: r.run_id, error: r.error }, exitCode: 1 };
        }
        return { payload: { count: r.count, run_ids: r.run_ids ?? [], status: r.status }, exitCode: 0 };
      }

      const ids: string[] = [];
      if (singleId) ids.push(singleId);
      for (const id of batchIds) if (id && !ids.includes(id)) ids.push(id);
      if (allPending) {
        const pending = auditUnverified(db, { agentId, workspacePath: cwd }) as unknown as {
          unverified: Array<{ run_id: string }>;
        };
        for (const i of pending.unverified) if (!ids.includes(i.run_id)) ids.push(i.run_id);
      }

      if (ids.length === 0) {
        throw new Error('memory_verify requires run_id, run_ids[], or allPending:true');
      }

      const verifyResults = ids.map((runId) => {
        const r = markVerified(db, {
          runId,
          agentId,
          workspacePath: verifyWorkspace,
          artifact: verifyArtifact,
          message: verifyMessage,
          status: verifyStatus,
        }) as MarkVerifiedResult;
        return r.ok
          ? { run_id: r.run_id, status: r.status }
          : { run_id: r.run_id, error: r.error };
      });

      const allOk = verifyResults.every((r) => !('error' in r));
      const payload = verifyResults.length === 1 ? verifyResults[0] : { count: verifyResults.length, results: verifyResults };
      return { payload, exitCode: allOk ? 0 : 1 };
    }
case 'digest': {
      const digestParams: Record<string, unknown> = {
        retention_days: (request['retention_days'] as number | undefined) ?? 90,
        refinement_handoff_retention_days: (request['refinement_handoff_retention_days'] as number | undefined) ?? 7,
        refinement_done_retention_days: (request['refinement_done_retention_days'] as number | undefined) ?? 30,
        operational_retention_days: (request['operational_retention_days'] as number | undefined) ?? 90,
        pressure_age_days: (request['pressure_age_days'] as number | undefined) ?? 1,
      };
      if (request['workspace'] || request['workspace_path']) {
        digestParams['workspace'] = request['workspace'] ?? request['workspace_path'];
      }
      if (request['artifact']) digestParams['artifact'] = request['artifact'];
      if (request['dry_run']) digestParams['dry_run'] = true;
      const result = digest(db, digestParams);

      const payload: Record<string, unknown> = result.dry_run
        ? {
            dry_run: true,
            would_archive: result.would_archive,
            would_prune_old: result.would_prune_old,
            would_prune_locks: result.would_prune_locks,
            would_prune_refinements: result.would_prune_refinements,
            would_prune_runs: result.would_prune_runs,
            pressure_age_days: result.pressure_age_days,
            stale_pending_runs: result.stale_pending_runs,
            stale_open_signals: result.stale_open_signals,
            stale_missing_refs: result.stale_missing_refs,
            pressure_samples: result.pressure_samples,
          }
        : {
            archived_memories: result.archived_memories,
            pruned_old: result.pruned_old,
            pruned_locks: result.pruned_locks,
            pruned_refinements: result.pruned_refinements,
            pruned_runs: result.pruned_runs,
            fts_rebuilt: result.fts_rebuilt,
            pressure_age_days: result.pressure_age_days,
            stale_pending_runs: result.stale_pending_runs,
            stale_open_signals: result.stale_open_signals,
            stale_missing_refs: result.stale_missing_refs,
            pressure_samples: result.pressure_samples,
          };

      if (request['export_doc']) {
        try {
          const wsPath = (request['workspace_path'] as string | undefined) ?? cwd;
          const docDir = join(wsPath, '.octocode', 'memory-reports');
          mkdirSync(docDir, { recursive: true });
          const dateStr = new Date().toISOString().slice(0, 16).replace('T', '-').replace(':', '');
          const docPath = join(docDir, `memory-report-${dateStr}.md`);
          const mdContent = exportMemoryDoc(db, { workspace_path: wsPath });
          writeFileSync(resolve(docPath), mdContent, 'utf8');
          payload['doc_path'] = docPath;
        } catch (err) {
          payload['doc_warning'] = `Could not write doc: ${(err as Error).message}`;
        }
      }

      return { payload, exitCode: 0 };
    }
  }
  return null;
}
