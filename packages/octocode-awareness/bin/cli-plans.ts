import type { DatabaseSync } from 'node:sqlite';
import { forgetMemory, archiveMemories, restoreMemories } from '../src/memory.js';
import { deleteRefinement } from '../src/refinements.js';
import { countPlans, createPlan, getPlan, joinPlan, listPlans, registerPlanDocument, updatePlanStatus, type PlanStatus } from '../src/plans.js';
import { addTaskDependency, claimTask, createTask, getTask, heartbeatTaskClaim, countReadyTasks, countTasks, listReadyTasks, listTasks, releaseTaskClaim, submitTask, type PlanTaskStatus } from '../src/tasks.js';
import { exportHarness } from '../src/maintenance.js';
import { normalizeWorkspacePath } from '../src/git.js';
import { ParsedArgs } from './cli-model.js';
import { EmitOptions, die, emit, firstValue, listLimit, valuesFor } from './cli-routing.js';

export function requiredArg(args: ParsedArgs, key: string): string {
  const value = args[key];
  if (value == null || value === true || !String(value).trim()) {
    die(`--${key.replace(/_/g, '-')} is required`);
  }
  return String(value).trim();
}

export function cmdPlan(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const action = requiredArg(args, 'action');
  if (action === 'create') {
    // An explicit --workspace also fixes where the .octocode/plan scaffolding
    // is written; without it the docs default to the normalized repo root.
    const explicitWorkspace = args['workspace'] ? String(args['workspace']) : null;
    const result = createPlan(db, {
      name: requiredArg(args, 'name'),
      objective: requiredArg(args, 'objective'),
      leadAgentId: String(args['lead_agent_id'] ?? args['agent_id'] ?? process.env.OCTOCODE_AGENT_ID ?? '').trim(),
      workspacePath: explicitWorkspace ?? process.cwd(),
      docsPath: explicitWorkspace,
      artifact: args['artifact'] ? String(args['artifact']) : null,
    });
    if (opts.compact) {
      return emit({
        ok: true,
        plan_id: result.plan.plan_id,
        status: result.plan.status,
        document: result.document_path,
      }, 0, opts);
    }
    return emit({ db_path: dbPath, ...result }, 0, opts);
  }
  if (action === 'list') {
    const filters = {
      workspacePath: args['workspace'] ? String(args['workspace']) : null,
      artifact: args['artifact'] ? String(args['artifact']) : null,
      status: args['status'] ? String(args['status']).toUpperCase() as PlanStatus : null,
    };
    const totalCount = countPlans(db, filters);
    const plans = listPlans(db, { ...filters, limit: listLimit(args, opts.compact ? 5 : 20) });
    const projected = Boolean(args['full']) ? plans : plans.map((plan) => ({
      plan_id: plan.plan_id,
      name: plan.name,
      status: plan.status,
      lead_agent_id: plan.lead_agent_id,
      updated_at: plan.updated_at,
    }));
    return emit({
      db_path: dbPath,
      count: projected.length,
      total_count: totalCount,
      omitted_count: Math.max(0, totalCount - projected.length),
      plans: projected,
    }, 0, opts);
  }
  const planId = requiredArg(args, 'plan_id');
  if (action === 'show') {
    const plan = getPlan(db, planId);
    return plan
      ? emit({ db_path: dbPath, plan }, 0, opts)
      : emit({ db_path: dbPath, error: `plan not found: ${planId}` }, 1, opts);
  }
  if (action === 'join') {
    const member = joinPlan(db, { planId, agentId: String(args['agent_id'] ?? process.env.OCTOCODE_AGENT_ID ?? '').trim() });
    if (opts.compact) return emit({ ok: true, plan_id: planId, agent_id: member.agent_id }, 0, opts);
    return emit({ db_path: dbPath, plan_id: planId, member }, 0, opts);
  }
  if (action === 'doc') {
    const document = registerPlanDocument(db, {
      planId,
      agentId: String(args['agent_id'] ?? process.env.OCTOCODE_AGENT_ID ?? '').trim(),
      relativePath: valuesFor(args, 'path')[0] ?? '',
      title: requiredArg(args, 'title'),
    });
    if (opts.compact) return emit({ ok: true, plan_id: planId, path: document.relative_path, title: document.title }, 0, opts);
    return emit({ db_path: dbPath, plan_id: planId, document }, 0, opts);
  }
  if (action === 'status') {
    const status = requiredArg(args, 'status').toUpperCase() as PlanStatus;
    if (!['DRAFT', 'ACTIVE', 'PAUSED', 'COMPLETED', 'CANCELLED'].includes(status)) {
      die('--status must be DRAFT, ACTIVE, PAUSED, COMPLETED, or CANCELLED');
    }
    const plan = updatePlanStatus(db, {
      planId,
      status,
      agentId: String(args['agent_id'] ?? process.env.OCTOCODE_AGENT_ID ?? '').trim(),
    });
    if (opts.compact) return emit({ ok: true, plan_id: plan.plan_id, status: plan.status }, 0, opts);
    return emit({ db_path: dbPath, plan }, 0, opts);
  }
  return emit({ db_path: dbPath, error: `unknown plan action: ${action}` }, 1, opts);
}

export function cmdTask(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const action = requiredArg(args, 'action');
  const agentId = String(args['agent_id'] ?? args['created_by'] ?? process.env.OCTOCODE_AGENT_ID ?? '').trim();
  if (action === 'create') {
    const result = createTask(db, {
      planId: requiredArg(args, 'plan_id'),
      title: requiredArg(args, 'title'),
      reasoning: requiredArg(args, 'reasoning'),
      acceptanceCriteria: requiredArg(args, 'acceptance'),
      paths: valuesFor(args, 'path'),
      createdBy: agentId,
      priority: args['priority'] == null ? undefined : Number(args['priority']),
      dependsOn: valuesFor(args, 'depends_on'),
    });
    if (opts.compact) {
      return emit({
        ok: true,
        task_id: result.task.task_id,
        plan_id: result.task.plan_id,
        status: result.task.status,
        path_count: result.task.paths.length,
      }, 0, opts);
    }
    return emit({ db_path: dbPath, ...result }, 0, opts);
  }
  if (action === 'list' || action === 'ready') {
    const rawTaskWs = args['workspace'] ? String(args['workspace']) : null;
    const filters = {
        planId: args['plan_id'] ? String(args['plan_id']) : null,
        status: args['status'] ? String(args['status']).toUpperCase() as PlanTaskStatus : null,
        agentId: args['agent_id'] ? agentId : null,
        workspacePath: rawTaskWs ? normalizeWorkspacePath(rawTaskWs, rawTaskWs) : null,
      };
    const limit = listLimit(args, opts.compact ? 5 : 20);
    const totalCount = action === 'ready'
      ? countReadyTasks(db, { planId: filters.planId, workspacePath: filters.workspacePath })
      : countTasks(db, filters);
    const tasks = action === 'ready'
      ? listReadyTasks(db, { planId: filters.planId, workspacePath: filters.workspacePath, limit })
      : listTasks(db, { ...filters, limit });
    const projected = Boolean(args['full']) ? tasks : tasks.map((task) => {
      const paths = opts.compact ? task.paths.slice(0, 3) : task.paths;
      const dependencies = opts.compact ? task.dependencies.slice(0, 3) : task.dependencies;
      return {
        task_id: task.task_id,
        plan_id: task.plan_id,
        title: task.title,
        status: task.status,
        priority: task.priority,
        paths,
        ...(opts.compact ? {
          path_count: task.paths.length,
          path_omitted_count: Math.max(0, task.paths.length - paths.length),
          dependency_count: task.dependencies.length,
          dependency_omitted_count: Math.max(0, task.dependencies.length - dependencies.length),
        } : {}),
        dependencies,
        claim: task.claim ? {
          run_id: task.claim.run_id,
          agent_id: task.claim.agent_id,
          expires_at: task.claim.expires_at,
        } : null,
      };
    });
    return emit({
      db_path: dbPath,
      count: projected.length,
      total_count: totalCount,
      omitted_count: Math.max(0, totalCount - projected.length),
      tasks: projected,
    }, 0, opts);
  }
  let taskId = args['task_id'] ? String(args['task_id']) : '';
  if (action === 'claim' && Boolean(args['next'])) {
    const planId = requiredArg(args, 'plan_id');
    taskId = listReadyTasks(db, { planId })[0]?.task_id ?? '';
    if (!taskId) return emit({ db_path: dbPath, error: `no ready tasks in plan ${planId}` }, 1, opts);
  }
  if (!taskId) die('--task-id is required');
  if (action === 'show') {
    const task = getTask(db, taskId);
    return task
      ? emit({ db_path: dbPath, task }, 0, opts)
      : emit({ db_path: dbPath, error: `task not found: ${taskId}` }, 1, opts);
  }
  if (action === 'depend') {
    const dependencies = valuesFor(args, 'depends_on');
    if (dependencies.length === 0) die('task depend requires at least one --depends-on');
    for (const dependsOnTaskId of dependencies) {
      addTaskDependency(db, { taskId, dependsOnTaskId, agentId });
    }
    const task = getTask(db, taskId);
    if (opts.compact) return emit({ ok: true, task_id: taskId, status: task?.status, dependency_count: task?.dependencies.length }, 0, opts);
    return emit({ db_path: dbPath, task }, 0, opts);
  }
  const leaseMinutes = args['lease_minutes'] == null ? undefined : Number(args['lease_minutes']);
  if (leaseMinutes != null && (leaseMinutes < 1 || leaseMinutes > 60)) die('--lease-minutes must be between 1 and 60');
  if (action === 'claim') {
    const result = claimTask(db, {
      taskId,
      agentId,
      leaseMs: leaseMinutes == null ? undefined : leaseMinutes * 60_000,
      testPlan: args['test_plan'] ? String(args['test_plan']) : undefined,
    });
    const exitCode = result.ok ? 0 : result.error.startsWith('task is already claimed by ') ? 2 : 1;
    if (opts.compact && result.ok) {
      return emit({
        ok: true,
        task_id: result.task.task_id,
        run_id: result.run.run_id,
        status: result.run.status,
        expires_at: result.claim.expires_at,
      }, 0, opts);
    }
    return emit({ db_path: dbPath, ...result }, exitCode, opts);
  }
  const runId = firstValue(args, 'run_id') ?? '';
  if (!runId) die('--run-id is required');
  if (action === 'heartbeat') {
    const claim = heartbeatTaskClaim(db, {
      taskId, runId, agentId,
      leaseMs: leaseMinutes == null ? undefined : leaseMinutes * 60_000,
    });
    if (opts.compact) return emit({ ok: true, task_id: taskId, run_id: runId, status: 'ACTIVE', expires_at: claim.expires_at }, 0, opts);
    return emit({ db_path: dbPath, claim }, 0, opts);
  }
  if (action === 'submit') {
    const result = submitTask(db, {
      taskId, runId, agentId,
      message: args['message'] ? String(args['message']) : undefined,
    });
    if (opts.compact) return emit({ ok: true, task_id: taskId, run_id: runId, status: result.run.status }, 0, opts);
    return emit({ db_path: dbPath, ...result }, 0, opts);
  }
  if (action === 'release') {
    const task = releaseTaskClaim(db, {
      taskId, runId, agentId,
      blockedReason: args['blocked_reason'] ? String(args['blocked_reason']) : null,
    });
    if (opts.compact) return emit({ ok: true, task_id: task.task_id, status: task.status }, 0, opts);
    return emit({ db_path: dbPath, task }, 0, opts);
  }
  return emit({ db_path: dbPath, error: `unknown task action: ${action}` }, 1, opts);
}

export function cmdForget(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawIds = args['memory_id'];
  const memoryIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const rawTags = [args['tag'], args['tags']].flatMap((v) =>
    Array.isArray(v) ? v : v && v !== true ? [String(v)] : []);
  const tags = rawTags;
  const result = forgetMemory(db, {
    memoryIds,
    tags,
    before: args['before'] ? String(args['before']) : undefined,
    maxImportance: args['max_importance'] ? parseInt(String(args['max_importance']), 10) : undefined,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    dryRun: Boolean(args['dry_run']),
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

export function cmdMemoryLifecycle(
  db: DatabaseSync,
  args: ParsedArgs,
  dbPath: string,
  opts: EmitOptions,
  action: 'archive' | 'restore',
): number {
  const rawIds = args['memory_id'];
  const memoryIds = Array.isArray(rawIds) ? rawIds.map(String) : rawIds ? [String(rawIds)] : [];
  if (memoryIds.length === 0) die('--memory-id is required');
  const params = {
    memoryIds,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    dryRun: Boolean(args['dry_run']),
  };
  const result = action === 'archive'
    ? archiveMemories(db, params)
    : restoreMemories(db, params);
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

export function cmdRefineDelete(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawIds = args['refinement_id'];
  const refinementIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  if (refinementIds.length === 0) return emit({ error: '--refinement-id is required' }, 1, opts);
  const result = deleteRefinement(db, {
    refinementIds,
    workspacePath: args['workspace'] ? String(args['workspace']) : undefined,
    artifact: args['artifact'] ? String(args['artifact']) : undefined,
    dryRun: Boolean(args['dry_run']),
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

export function cmdExportHarness(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const result = exportHarness(db, {
    limit: args['limit'] ? parseInt(String(args['limit']), 10) : undefined,
    min_importance: args['min_importance'] ? parseInt(String(args['min_importance']), 10) : undefined,
    workspace_path: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}
