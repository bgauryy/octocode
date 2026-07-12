import type { DatabaseSync } from 'node:sqlite';
import { getWorkspaceStatus } from './maintenance.js';
import { getRefinements } from './refinements.js';
import { queryAwareness } from './repo-context.js';
import { auditUnverified } from './verify.js';
import type { AwarenessToolOperation, AwarenessToolOperationContext, AwarenessToolOperationResult } from './tool-operations.js';

export function runWorkspaceOperation(
  db: DatabaseSync,
  operation: AwarenessToolOperation,
  request: Record<string, unknown>,
  context: AwarenessToolOperationContext,
): AwarenessToolOperationResult | null {
  const cwd = context.cwd ?? process.cwd();
  const agentId = context.agentId ?? 'agent';
  switch (operation) {
case 'workspace_status': {
      const workspacePath = (request['workspace_path'] as string | undefined) ?? cwd;
      const result = getWorkspaceStatus(db, {
        workspace_path: workspacePath,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        cwd,
      });
      const requestedLimit = request['limit'] as number | undefined;
      const workboard = queryAwareness(db, {
        view: 'workboard',
        workspacePath,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        limit: Math.max(1, Math.min(requestedLimit ?? 5, 20)),
        cwd,
      });
      const filesUnderWork = workboard.rows
        .filter((row) => row['column'] === 'FilesUnderWork')
        .map((row) => ({
          path: row['path'],
          peer_count: row['peer_count'],
          agents: row['agents'],
          run_ids: row['run_ids'],
          task_ids: row['task_ids'],
          plan_ids: row['plan_ids'],
          plans: row['plans'],
          reasons: row['reasons'],
          omitted_peer_count: row['omitted_peer_count'],
          locked: row['locked'],
          ...(row['lock_agent_id'] ? { lock_agent_id: row['lock_agent_id'] } : {}),
          ...(row['lock_expires_at'] ? { lock_expires_at: row['lock_expires_at'] } : {}),
        }));
      const payload: Record<string, unknown> = {
        active_memories: result.active_memories,
        pending_runs: result.pending_runs,
        active_runs: result.active_runs,
        planning: {
          active_plans: result.active_plans,
          ready_tasks: result.ready_tasks,
          in_progress_tasks: result.in_progress_tasks,
          verify_tasks: result.verify_tasks,
        },
        actionable_refinements: result.actionable_refinements,
        all_open_refinements: result.all_open_refinements,
        lock_count: result.lock_count,
      };
      if (filesUnderWork.length > 0) payload['files_under_work'] = filesUnderWork;
      const shownLocks = result.locks.slice(0, Math.max(1, Math.min(requestedLimit ?? 5, 20)));
      payload['lock_shown_count'] = shownLocks.length;
      payload['lock_omitted_count'] = Math.max(0, result.lock_count - shownLocks.length);
      if (shownLocks.length > 0) {
        payload['locks'] = shownLocks.map((l) => ({
          file: l.file_path,
          run_id: l.run_id,
          agent: l.agent_id,
          type: l.lock_type,
          since: l.acquired_at,
          ...(l.expires_at ? { expires: l.expires_at } : {}),
        }));
      }
      return { payload, exitCode: 0 };
    }
case 'refine_get': {
      const result = getRefinements(db, {
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        repo: request['repo'] as string | undefined,
        states: request['state'] ? [(request['state'] as string)] : undefined,
        includeHandoffs: Boolean(request['include_handoffs']),
        limit: (request['limit'] as number | undefined) ?? 5,
        cwd,
      }) as unknown as {
        refinements: Array<{
          refinement_id: string;
          state: string;
          remember: string;
          files?: string[];
          repo?: string;
        }>;
      };
      const refinements = result.refinements.map((r) => {
        const lean: Record<string, unknown> = {
          refinement_id: r.refinement_id,
          state: r.state,
          fix: r.remember,
        };
        if (r.files?.length) lean['files'] = r.files;
        if (r.repo) lean['repo'] = r.repo;
        return lean;
      });
      return { payload: { count: refinements.length, refinements }, exitCode: 0 };
    }
case 'verify_audit': {
      const result = auditUnverified(db, {
        agentId,
        workspacePath: cwd,
        olderThanDays: request['older_than_days'] as number | undefined,
        origins: Array.isArray(request['origin'])
          ? request['origin'] as Array<'TASK' | 'WORK' | 'HOOK'>
          : undefined,
        before: request['before'] as string | undefined,
      }) as unknown as {
        unverified: Array<{ run_id: string; test_plan: string; target_files?: string[] }>;
        stale_active: Array<{ run_id: string; agent_id: string; age_hours: number; rationale: string; target_files?: string[] }>;
        count: number;
      };
      const pending = result.unverified.map((i) => {
        const lean: Record<string, unknown> = { run_id: i.run_id, test_plan: i.test_plan };
        if (i.target_files?.length) lean['files'] = i.target_files;
        return lean;
      });
      const stale = (result.stale_active ?? []).map((i) => {
        const lean: Record<string, unknown> = {
          run_id: i.run_id,
          agent_id: i.agent_id,
          age_hours: i.age_hours,
          reason: `ACTIVE run with no live file presence or task claim (orphaned session) - ${i.rationale}`,
        };
        if (i.target_files?.length) lean['files'] = i.target_files;
        return lean;
      });
      const payload: Record<string, unknown> = { count: result.count, pending };
      if (stale.length > 0) payload['stale_active'] = stale;
      return { payload, exitCode: result.count ? 1 : 0 };
    }
  }
  return null;
}
