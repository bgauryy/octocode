import path from 'node:path';
import type { Task } from '@octocodeai/agent-contracts/entities';
import { openAwarenessStore } from './open.js';

export type ExternalPlanScope = 'auto' | 'session' | 'shared';

export interface ExternalPlanProjectionStep {
  id: string;
  text: string;
  status: 'todo' | 'doing' | 'done';
  dependsOnStepIds?: string[];
  paths?: string[];
  reasoning?: string;
  acceptance?: string;
  checkCommand?: string;
  awarenessTaskId?: string;
}

export interface ExternalPlanProjectionInput {
  requestedScope: ExternalPlanScope;
  workspace: string;
  sourceKind?: string;
  sourcePlanKey: string;
  awarenessPlanId?: string;
  title: string;
  goal?: string;
  rfcPath?: string;
  rfcRevision?: string;
  agentId: string;
  steps: ExternalPlanProjectionStep[];
}

export interface ExternalPlanProjectionResult {
  scope: 'session' | 'shared';
  adopted: boolean;
  awarenessPlanId?: string;
  taskIdsByStepId?: Record<string, string>;
}

export interface ObservedCheckReceipt {
  command: string;
  status: 'SUCCESS' | 'FAILED';
  message: string;
}

export interface ExternalPlanCompletionResult {
  task: Task;
  verified: boolean;
}

type CompletionStore = Pick<ReturnType<typeof openAwarenessStore>, 'getTask' | 'doneTask' | 'markCheck' | 'close'>;

function normalizedPaths(workspace: string, paths: string[] | undefined): Set<string> {
  return new Set((paths ?? []).map((candidate) => path.resolve(workspace, candidate)));
}

function safelyAdoptableClaim(workspace: string, steps: ExternalPlanProjectionStep[], claimed: Task[]): Task | undefined {
  if (steps.length !== 1 || claimed.length !== 1) return undefined;
  const step = steps[0]!;
  const task = claimed[0]!;
  const stepPaths = normalizedPaths(workspace, step.paths);
  const taskPaths = normalizedPaths(workspace, task.paths);
  return [...stepPaths].some((candidate) => taskPaths.has(candidate)) || task.title === step.text ? task : undefined;
}

export function completeExternalPlanTask(
  input: { workspace: string; taskId: string; agentId: string; receipt?: ObservedCheckReceipt },
  openStore: (workspace: string) => CompletionStore = (workspace) => openAwarenessStore({ workspace }),
): ExternalPlanCompletionResult {
  const aw = openStore(input.workspace);
  try {
    const current = aw.getTask(input.taskId);
    if (current.status === 'DONE' && current.verifiedAt) return { task: current, verified: true };
    if (current.status !== 'IN_PROGRESS' || current.agentId !== input.agentId || !current.runId) {
      throw new Error(`task ${current.taskId} is not actively claimed by ${input.agentId}`);
    }
    if (current.checkCommand) {
      if (!input.receipt) throw new Error(`task ${current.taskId} requires an observed check receipt`);
      if (input.receipt.command.trim() !== current.checkCommand.trim()) throw new Error(`receipt command must match declared check command: ${current.checkCommand}`);
    }
    if (input.receipt && !input.receipt.message.trim()) throw new Error('receipt message is required');

    const completed = aw.doneTask({ taskId: current.taskId, runId: current.runId, agentId: input.agentId });
    const marked = aw.markCheck({
      doneAt: completed.updatedAt,
      taskId: current.taskId,
      runId: current.runId,
      agentId: input.agentId,
      message: input.receipt?.message.trim() || 'Completed without a configured check command',
      status: input.receipt?.status ?? 'SUCCESS',
    });
    return { task: marked, verified: input.receipt?.status !== 'FAILED' };
  } finally {
    aw.close();
  }
}

export function finalizeExternalPlan(input: { workspace: string; planId: string; agentId: string }): boolean {
  const aw = openAwarenessStore({ workspace: input.workspace });
  try {
    const tasks = aw.listTasks({ planId: input.planId });
    if (tasks.length === 0 || tasks.some((task) => task.status !== 'DONE' || !task.verifiedAt)) return false;
    aw.donePlan({ planId: input.planId, agentId: input.agentId });
    return true;
  } finally {
    aw.close();
  }
}

/** Resolve or materialize an external host's execution plan in the shared ledger. */
export function projectExternalPlan(input: ExternalPlanProjectionInput): ExternalPlanProjectionResult {
  if (input.requestedScope === 'session') return { scope: 'session', adopted: false };
  const sourceKind = input.sourceKind?.trim() || 'external-agent';
  const aw = openAwarenessStore({ workspace: input.workspace });
  try {
    if (input.awarenessPlanId && input.steps.length > 0 && input.steps.every((step) => step.awarenessTaskId)) {
      const taskIdsByStepId: Record<string, string> = {};
      for (const step of input.steps) {
        const task = aw.getTask(step.awarenessTaskId!);
        if (task.planId !== input.awarenessPlanId) throw new Error(`mapped task ${task.taskId} belongs to another plan`);
        taskIdsByStepId[step.id] = task.taskId;
      }
      const plan = aw.getPlan(input.awarenessPlanId);
      if (plan.sourceKind !== sourceKind || plan.sourceKey !== input.sourcePlanKey) {
        return { scope: 'shared', adopted: true, awarenessPlanId: input.awarenessPlanId, taskIdsByStepId };
      }
    }

    const adoptable = safelyAdoptableClaim(input.workspace, input.steps, aw.listTasks({ status: 'IN_PROGRESS', agentId: input.agentId }));
    if (adoptable) return { scope: 'shared', adopted: true, awarenessPlanId: adoptable.planId, taskIdsByStepId: { [input.steps[0]!.id]: adoptable.taskId } };
    if (input.requestedScope === 'auto') return { scope: 'session', adopted: false };
    if (input.steps.length === 0) throw new Error('shared plan requires at least one execution step');

    const graph = aw.materializePlanGraph({
      sourceKind,
      sourcePlanKey: input.sourcePlanKey,
      title: input.title,
      goal: input.goal,
      rfcPath: input.rfcPath,
      rfcRevision: input.rfcRevision,
      agentId: input.agentId,
      steps: input.steps.map((step, index) => ({
        sourceStepKey: step.id,
        title: step.text,
        paths: step.paths,
        reasoning: step.reasoning,
        acceptance: step.acceptance,
        checkCommand: step.checkCommand,
        dependsOnStepKeys: step.dependsOnStepIds,
        priority: input.steps.length - index,
      })),
    });
    const taskIdsByStepId = Object.fromEntries([...graph.tasks].map(([stepId, task]) => [stepId, task.taskId]));
    const active = input.steps.find((step) => step.status === 'doing');
    if (active) {
      const task = graph.tasks.get(active.id);
      if (!task) throw new Error(`missing materialized task for active step ${active.id}`);
      if (task.status === 'OPEN') aw.claimTask({ taskId: task.taskId, agentId: input.agentId });
      else if (task.status === 'IN_PROGRESS' && task.agentId !== input.agentId) throw new Error(`task ${task.taskId} belongs to ${task.agentId}`);
    }
    return { scope: 'shared', adopted: false, awarenessPlanId: graph.plan.planId, taskIdsByStepId };
  } finally {
    aw.close();
  }
}
