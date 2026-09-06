import type { Plan, PlanStatus, Task, TaskStatus } from '@octocodeai/agent-contracts/entities';
import { createPlan as createCanonicalPlan, getPlan as getCanonicalPlan, listPlans as listCanonicalPlans, updatePlanStatus } from '../plans.js';
import { claimTask as claimCanonicalTask, heartbeatTaskClaim, releaseTaskClaim, submitTask } from '../tasks-claims.js';
import { addTaskDependency, countReadyTasks, createTask, listReadyTasks, listTasks } from '../tasks-ready.js';
import { CoordinationBase } from './coordination-core.js';
import { planEntity, taskEntity } from './canonical-plan-task-entities.js';
import { normalizeWorkspacePath } from '../git.js';

function required(value: string | null | undefined, name: string): string {
  const normalized = value?.trim();
  if (!normalized) throw new Error(`${name} is required`);
  return normalized;
}

function canonicalWorkspace(workspace: string): string {
  return normalizeWorkspacePath(workspace, workspace) ?? workspace;
}

export abstract class CoordinationPlansTasks extends CoordinationBase {
  status(params: { staleAfterMs?: number } = {}): {
    dbPath: string; workspace: string; plans: number; activePlans: number; tasks: number;
    readyTasks: number; inProgressTasks: number; pendingChecks: number; verifyTasks: number;
    locks: number; work: number; memories: number; handoffs: number; agents: number;
    staleAgents: number; messages: number;
  } {
    const workspace = canonicalWorkspace(this.workspace);
    const tasks = listTasks(this.db, { workspacePath: workspace });
    const pendingChecks = this.auditChecks({}).pending.length;
    return {
      dbPath: this.dbPath,
      workspace: this.workspace,
      plans: listCanonicalPlans(this.db, { workspacePath: workspace }).length,
      activePlans: listCanonicalPlans(this.db, { workspacePath: workspace, status: 'ACTIVE' }).length,
      tasks: tasks.length,
      readyTasks: countReadyTasks(this.db, { workspacePath: workspace }),
      inProgressTasks: tasks.filter((task) => task.status === 'IN_PROGRESS').length,
      pendingChecks,
      verifyTasks: pendingChecks,
      locks: this.listLocks().length,
      work: this.listWork({}).length,
      memories: this.countMemories(),
      handoffs: this.countOpenHandoffs(),
      agents: this.countPresentAgents(params.staleAfterMs ?? 30_000),
      staleAgents: params.staleAfterMs ? this.countStaleAgents(params.staleAfterMs) : 0,
      messages: this.countSignals(),
    };
  }

  createPlan(params: { title: string; goal?: string | null; agentId: string }): Plan {
    return planEntity(createCanonicalPlan(this.db, {
      name: required(params.title, 'title'),
      objective: required(params.goal, 'goal'),
      leadAgentId: required(params.agentId, 'agent-id'),
      workspacePath: this.workspace,
    }).plan);
  }

  listPlans(status?: PlanStatus): Plan[] {
    return listCanonicalPlans(this.db, { workspacePath: canonicalWorkspace(this.workspace), status }).map(planEntity);
  }

  getPlan(planId: string): Plan {
    const plan = getCanonicalPlan(this.db, planId);
    if (!plan || plan.workspace_path !== canonicalWorkspace(this.workspace)) throw new Error(`plan not found: ${planId}`);
    return planEntity(plan);
  }

  donePlan(params: { planId: string; agentId: string }): Plan {
    this.getPlan(params.planId);
    return planEntity(updatePlanStatus(this.db, {
      planId: params.planId,
      status: 'COMPLETED',
      agentId: required(params.agentId, 'agent-id'),
    }));
  }

  addTask(params: {
    planId: string; title: string; filePath?: string | null; paths?: string | string[] | null;
    reasoning?: string | null; acceptance?: string | null; checkCommand?: string | null;
    dependsOn?: string | string[] | null; priority?: number; agentId: string;
  }): Task {
    this.getPlan(params.planId);
    const rawPaths = Array.isArray(params.paths) ? params.paths : params.paths ? [params.paths] : [];
    const paths = rawPaths.length > 0 ? rawPaths : params.filePath ? [params.filePath] : [];
    const dependencies = Array.isArray(params.dependsOn) ? params.dependsOn : params.dependsOn ? [params.dependsOn] : [];
    const task = createTask(this.db, {
      planId: params.planId,
      title: required(params.title, 'title'),
      paths,
      reasoning: required(params.reasoning, 'reasoning'),
      acceptanceCriteria: required(params.acceptance, 'acceptance'),
      createdBy: required(params.agentId, 'agent-id'),
      priority: params.priority,
      dependsOn: dependencies,
    }).task;
    if (params.checkCommand?.trim()) {
      this.db.prepare('UPDATE awareness_tasks SET check_command = ? WHERE task_id = ?')
        .run(params.checkCommand.trim(), task.task_id);
      task.check_command = params.checkCommand.trim();
    }
    return taskEntity(task);
  }

  addTaskDependency(params: { taskId: string; dependsOnTaskId: string; agentId: string }): Task {
    this.getTask(params.taskId);
    this.getTask(params.dependsOnTaskId);
    addTaskDependency(this.db, { ...params, agentId: required(params.agentId, 'agent-id') });
    return this.getTask(params.taskId);
  }

  listTasks(params: { planId?: string; status?: TaskStatus; agentId?: string } = {}): Task[] {
    return listTasks(this.db, { ...params, workspacePath: canonicalWorkspace(this.workspace) }).map(taskEntity);
  }

  listReadyTasks(params: { planId?: string; limit?: number } = {}): Task[] {
    return listReadyTasks(this.db, { ...params, workspacePath: canonicalWorkspace(this.workspace) }).map(taskEntity);
  }

  getTask(taskId: string): Task {
    const task = listTasks(this.db, { workspacePath: canonicalWorkspace(this.workspace) }).find((item) => item.task_id === taskId);
    if (!task) throw new Error(`task not found: ${taskId}`);
    return taskEntity(task);
  }

  claimTask(params: { taskId: string; agentId: string; leaseSeconds?: number }): Task {
    this.getTask(params.taskId);
    const claim = claimCanonicalTask(this.db, {
      taskId: params.taskId,
      agentId: required(params.agentId, 'agent-id'),
      leaseMs: params.leaseSeconds == null ? undefined : params.leaseSeconds * 1000,
    });
    if (!claim.ok) throw new Error(claim.error);
    return taskEntity(claim.task);
  }

  heartbeatTask(params: { taskId: string; runId: string; agentId: string; leaseSeconds?: number }): Task {
    this.getTask(params.taskId);
    heartbeatTaskClaim(this.db, {
      taskId: params.taskId,
      runId: required(params.runId, 'run-id'),
      agentId: required(params.agentId, 'agent-id'),
      leaseMs: params.leaseSeconds == null ? undefined : params.leaseSeconds * 1000,
    });
    return this.getTask(params.taskId);
  }

  releaseTask(params: { taskId: string; runId: string; agentId: string; blockedReason?: string | null }): Task {
    this.getTask(params.taskId);
    return taskEntity(releaseTaskClaim(this.db, {
      taskId: params.taskId,
      runId: required(params.runId, 'run-id'),
      agentId: required(params.agentId, 'agent-id'),
      blockedReason: params.blockedReason,
    }));
  }

  doneTask(params: { taskId: string; runId: string; agentId: string }): Task {
    this.getTask(params.taskId);
    return taskEntity(submitTask(this.db, {
      taskId: params.taskId,
      runId: required(params.runId, 'run-id'),
      agentId: required(params.agentId, 'agent-id'),
    }).task);
  }
}
