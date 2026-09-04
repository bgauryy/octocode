import type { PiContext } from '../types.js';
import type { PlanCoordination, PlanDecision, PlanReviewComment, PlanStep, ReviewQuestion, ReviewState } from './active-plan.js';
import { activePlanScope, dependencyIndexes, displayStatus, getPlan, getPlanCoordination, getPlanReviewState, getPlanTurnsSinceUpdate, STALE_PLAN_TURNS } from './active-plan.js';
import { listPendingInteractionIds } from './interaction-broker.js';
import { escapePromptMetadata } from './prompt-safety.js';
import { openPersistentAwareness } from './storage-policy.js';

export interface PlanReadModelTaskV1 {
  id: string;
  index: number;
  text: string;
  activeText?: string;
  status: 'todo' | 'doing' | 'done' | 'blocked';
  dependsOn: number[];
  paths?: string[];
  reasoning?: string;
  acceptance?: string;
  checkCommand?: string;
  awarenessTaskId?: string;
}

export interface PlanReadModelV1 {
  version: 1;
  /** Stable plan identity — the `sourcePlanKey` from coordination, e.g. `pi-plan-<uuid>`. Stable across plan mutations and used to link the HTML page to a specific plan lifecycle. */
  planId: string;
  phase: ReviewState['phase'];
  revision?: string;
  acceptedRevision?: string;
  summary: { total: number; done: number; running: number; blocked: number };
  tasks: PlanReadModelTaskV1[];
  review: {
    branchSnapshotId: string;
    generation: number;
    rfcPath?: string;
    outcomeReason?: string;
    blockingQuestions: number;
    unresolvedComments: number;
    decisions: PlanDecision[];
    questions: ReviewQuestion[];
    comments: PlanReviewComment[];
  };
  coordination: {
    mode: PlanCoordination['mode'];
    sourcePlanKey: string;
    workspace: string;
    awarenessPlanId?: string;
    materializedRevision?: string;
  };
  authorization: { acceptReceiptId?: string; startReceiptId?: string };
  pendingInteractionIds: string[];
  runtime: { turnsSinceUpdate: number };
}

type SharedTaskStatus = 'OPEN' | 'CLAIMED' | 'IN_PROGRESS' | 'BLOCKED' | 'VERIFY' | 'DONE' | 'FAILED' | 'CANCELLED';

const SHARED_STATUS_TO_DISPLAY: Record<SharedTaskStatus, PlanReadModelTaskV1['status']> = {
  OPEN: 'todo',
  CLAIMED: 'doing',
  IN_PROGRESS: 'doing',
  BLOCKED: 'blocked',
  VERIFY: 'doing',
  DONE: 'done',
  FAILED: 'blocked',
  CANCELLED: 'blocked',
};

function sharedDisplayStatus(status: string | undefined): PlanReadModelTaskV1['status'] | undefined {
  return status && status in SHARED_STATUS_TO_DISPLAY
    ? SHARED_STATUS_TO_DISPLAY[status as SharedTaskStatus]
    : undefined;
}

export function buildPlanReadModel(input: {
  steps: PlanStep[];
  review: ReviewState;
  coordination: PlanCoordination;
  pendingInteractionIds?: string[];
  turnsSinceUpdate?: number;
  sharedTaskStatuses?: Readonly<Record<string, string | undefined>>;
}): PlanReadModelV1 {
  const tasks = input.steps.map((step, index) => ({
    id: step.id,
    index: index + 1,
    text: step.text,
    ...(step.activeForm ? { activeText: step.activeForm } : {}),
    status: sharedDisplayStatus(step.awarenessTaskId ? input.sharedTaskStatuses?.[step.awarenessTaskId] : undefined)
      ?? displayStatus(step, input.steps),
    dependsOn: dependencyIndexes(step, input.steps),
    ...(step.paths?.length ? { paths: [...step.paths] } : {}),
    ...(step.reasoning ? { reasoning: step.reasoning } : {}),
    ...(step.acceptance ? { acceptance: step.acceptance } : {}),
    ...(step.checkCommand ? { checkCommand: step.checkCommand } : {}),
    ...(step.awarenessTaskId ? { awarenessTaskId: step.awarenessTaskId } : {}),
  }));
  return {
    version: 1,
    planId: input.coordination.sourcePlanKey,
    phase: input.review.phase,
    ...(input.review.revision ? { revision: input.review.revision } : {}),
    ...(input.review.acceptedRevision ? { acceptedRevision: input.review.acceptedRevision } : {}),
    summary: {
      total: tasks.length,
      done: tasks.filter((task) => task.status === 'done').length,
      running: tasks.filter((task) => task.status === 'doing').length,
      blocked: tasks.filter((task) => task.status === 'blocked').length,
    },
    tasks,
    review: {
      branchSnapshotId: input.review.branchSnapshotId,
      generation: input.review.generation,
      ...(input.review.rfcPath ? { rfcPath: input.review.rfcPath } : {}),
      ...(input.review.outcomeReason ? { outcomeReason: input.review.outcomeReason } : {}),
      blockingQuestions: input.review.blockingQuestions.filter((question) => question.blocking && !question.answer).length,
      unresolvedComments: input.review.comments.filter((comment) => comment.blocking && !comment.resolved).length,
      decisions: input.review.decisions.map((decision) => ({ ...decision })),
      questions: input.review.blockingQuestions.map((question) => ({ ...question })),
      comments: input.review.comments.map((comment) => ({ ...comment })),
    },
    coordination: {
      mode: input.coordination.mode,
      sourcePlanKey: input.coordination.sourcePlanKey,
      workspace: input.coordination.coordinationWorkspace,
      ...(input.coordination.awarenessPlanId ? { awarenessPlanId: input.coordination.awarenessPlanId } : {}),
      ...(input.coordination.materializedRevision ? { materializedRevision: input.coordination.materializedRevision } : {}),
    },
    authorization: {
      ...(input.review.acceptAuthorizationReceiptId ? { acceptReceiptId: input.review.acceptAuthorizationReceiptId } : {}),
      ...(input.review.startAuthorizationReceiptId ? { startReceiptId: input.review.startAuthorizationReceiptId } : {}),
    },
    pendingInteractionIds: [...new Set(input.pendingInteractionIds ?? [])].sort(),
    runtime: { turnsSinceUpdate: input.turnsSinceUpdate ?? 0 },
  };
}

/** The sole stateful production loader for every user-visible plan projection. */
export function getCurrentPlanReadModel(ctx: PiContext | undefined, scope = activePlanScope(ctx)): PlanReadModelV1 {
  let pendingInteractionIds: string[] = [];
  if (ctx) {
    try { pendingInteractionIds = listPendingInteractionIds(ctx); } catch { /* presentation remains available when continuity storage is unavailable */ }
  }
  const steps = getPlan(scope);
  const coordination = getPlanCoordination(scope);
  const sharedTaskStatuses: Record<string, string> = {};
  const mappedTaskIds = steps.flatMap((step) => step.awarenessTaskId ? [step.awarenessTaskId] : []);
  if (coordination.awarenessPlanId && mappedTaskIds.length > 0) {
    let awareness: ReturnType<typeof openPersistentAwareness> | undefined;
    try {
      awareness = openPersistentAwareness({ workspace: coordination.coordinationWorkspace });
      for (const taskId of mappedTaskIds) sharedTaskStatuses[taskId] = awareness.getTask(taskId).status;
    } catch {
      // Keep rendering the branch-local snapshot when the shared DB is unavailable.
    } finally {
      awareness?.close();
    }
  }
  return buildPlanReadModel({
    steps,
    review: getPlanReviewState(scope),
    coordination,
    pendingInteractionIds,
    turnsSinceUpdate: getPlanTurnsSinceUpdate(scope),
    sharedTaskStatuses,
  });
}

const CONTEXT_MARK: Record<PlanReadModelTaskV1['status'], string> = { todo: '[ ]', doing: '[~]', done: '[x]', blocked: '[!]' };

/** Pure prompt/context renderer. It performs no reads and cannot mutate plan state. */
export function renderPlanContext(model: PlanReadModelV1): string {
  if (model.tasks.length === 0 && model.pendingInteractionIds.length === 0) return '';
  const inputGate = model.pendingInteractionIds.length > 0
    ? `input-needed: pending interactions ${model.pendingInteractionIds.map(escapePromptMetadata).join(', ')} — do not continue plan execution until the requested user input is resolved.`
    : undefined;
  if (model.tasks.length === 0) return ['<active_plan>', inputGate!, 'next: awaiting user input', '</active_plan>'].join('\n');
  const metadata = [
    inputGate,
    `state: phase=${model.phase} snapshot=${escapePromptMetadata(model.review.branchSnapshotId)} generation=${model.review.generation}`,
    model.review.rfcPath ? `rfc: ${escapePromptMetadata(model.review.rfcPath)}${model.revision ? ` displayed=${escapePromptMetadata(model.revision)}` : ''}${model.acceptedRevision ? ` accepted=${escapePromptMetadata(model.acceptedRevision)}` : ''}` : undefined,
    `coordination: mode=${model.coordination.mode}${model.coordination.awarenessPlanId ? ` awareness-plan=${escapePromptMetadata(model.coordination.awarenessPlanId)}` : ''}${model.coordination.materializedRevision ? ` materialized=${escapePromptMetadata(model.coordination.materializedRevision)}` : ''}`,
    ...model.review.decisions.map((decision) => `decision: ${escapePromptMetadata(decision.q)} => ${escapePromptMetadata(decision.a)}`),
    ...model.review.questions.map((question) => `question${question.answer ? '-answered' : '-blocking'}: ${escapePromptMetadata(question.prompt)}${question.answer ? ` => ${escapePromptMetadata(question.answer)}` : ''}`),
    ...model.review.comments.filter((comment) => !comment.resolved).map((comment) => `review-blocker: ${escapePromptMetadata(comment.body)}${comment.section ? ` section=${escapePromptMetadata(comment.section)}` : ''}`),
  ].filter((line): line is string => Boolean(line));
  const rows = model.tasks.map((task) => `${CONTEXT_MARK[task.status]} ${task.index}. ${escapePromptMetadata(task.text)}${task.dependsOn.length ? ` (needs ${task.dependsOn.join(',')})` : ''}`);
  const contracts = model.tasks.flatMap((task) => {
    const fields = [
      task.paths?.length ? `paths=${task.paths.map(escapePromptMetadata).join(',')}` : undefined,
      task.reasoning ? `reason=${escapePromptMetadata(task.reasoning)}` : undefined,
      task.acceptance ? `accept=${escapePromptMetadata(task.acceptance)}` : undefined,
      task.checkCommand ? `check=${escapePromptMetadata(task.checkCommand)}` : undefined,
      task.awarenessTaskId ? `awareness-task=${escapePromptMetadata(task.awarenessTaskId)}` : undefined,
    ].filter((field): field is string => Boolean(field));
    return fields.length ? [`contract ${task.index}: ${fields.join(' | ')}`] : [];
  });
  const executionAllowed = model.phase === 'executing' || model.phase === 'verifying';
  if (!executionAllowed) {
    return ['<active_plan>', `This task breakdown is in ${model.phase.replace('_', ' ')} (0/${model.tasks.length} done). Implementation has not started; do not execute or start any step before the separate Start transition.`, ...metadata, ...rows, ...contracts, 'next: awaiting user approval', '</active_plan>'].join('\n');
  }
  const doing = model.tasks.filter((task) => task.status === 'doing');
  const current = doing[0] ?? model.tasks.find((task) => task.status === 'todo');
  const runnable = model.tasks.filter((task) => task.status === 'todo' && task.dependsOn.every((index) => model.tasks[index - 1]?.status === 'done'));
  const nudges = doing.length === 0 && model.summary.done < model.summary.total
    ? ['note: no step is in progress — mark the next runnable step with plan(start:N) so unfinished work has an active owner.']
    : doing.length > 0 && runnable.length > 0
      ? [`parallel-ready: ${runnable.map((task) => `${task.index}. ${escapePromptMetadata(task.activeText ?? task.text)}`).join(' | ')} — start independent lanes with plan(start:N) before spawning/batching, or leave them todo if they depend on the current decision.`]
      : [];
  if (model.summary.done < model.summary.total && model.runtime.turnsSinceUpdate >= STALE_PLAN_TURNS) {
    nudges.push(`note: this plan has not been updated in ${STALE_PLAN_TURNS}+ turns — advance it (plan start/complete), add/remove changed scope, or clear it if the work is done or abandoned.`);
  }
  const next = doing.length > 1
    ? `now: ${doing.map((task) => escapePromptMetadata(task.activeText ?? task.text)).join(' | ')}`
    : current ? `next: ${escapePromptMetadata(current.activeText ?? current.text)}` : 'next: (all steps done — verify, then plan clear)';
  return ['<active_plan>', `Your current task breakdown (${model.summary.done}/${model.summary.total} done). Execute active steps; start independent parallel lanes with plan(start:N); advance and clear via plan(start/complete/add/remove/clear).`, ...metadata, ...rows, ...contracts, next, ...nudges, '</active_plan>'].join('\n');
}

export function renderPlanReadModel(model: PlanReadModelV1, format: 'terminal' | 'browser' | 'rpc'): string | PlanReadModelV1 {
  if (format === 'rpc') return model;
  const rows = model.tasks.map((task) => `${task.index}. [${task.status}] ${task.text}`);
  const inputGate = model.pendingInteractionIds.length > 0 ? `Input needed · ${model.pendingInteractionIds.join(', ')}` : undefined;
  const text = [inputGate, `Plan ${model.summary.done}/${model.summary.total} · ${model.phase}`, ...rows].filter((line): line is string => Boolean(line)).join('\n');
  if (format === 'terminal') return text;
  const escaped = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  return `<section data-plan-read-model="1" data-revision="${model.revision ?? ''}"><pre>${escaped}</pre></section>`;
}
