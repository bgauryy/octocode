/**
 * plan-executor — step-level mutations.
 * addStep, startStep, completeStep, removeStep, restorePlanSteps, activatePlan, hasActivePlanWork
 */

import {
  getPlan,
  getPlanLifecycle,
  phaseAllowsExecution,
  planSetRawSteps,
  planSetRawLifecycle,
  planDeleteClearedScope,
  planMarkUpdated,
  planRunPersist,
  planRunEnsureLoaded,
} from './plan-store.js';
import { dependencyIdsFromIndexes, MAX_PLAN_STEPS, normalizeInput } from './plan-normalization.js';
import type { PlanScope } from './plan-scope.js';
import { depsMet } from './plan-types.js';
import type { PlanStep, PlanStepInput } from './plan-types.js';

/** Promote an accepted draft and start its first runnable step. */
export function activatePlan(scope: PlanScope): PlanStep[] {
  const list = getPlan(scope).slice();
  if (list.length > 0 && !list.some((step) => step.status === 'doing')) {
    const next = list.findIndex((step) => step.status === 'todo' && depsMet(step, list));
    if (next >= 0) list[next] = { ...list[next]!, status: 'doing' };
  }
  planSetRawSteps(scope, list);
  planSetRawLifecycle(scope, 'executing');
  planMarkUpdated(scope);
  planRunPersist(scope);
  return list;
}

/**
 * Whether the scope has an actively owned in-progress step. Auto-compaction uses
 * this stricter signal so stale todo/blocked plan state after a finished turn
 * cannot trigger a surprise compaction; it should fire only while work is live.
 */
export function hasActivePlanWork(scope: PlanScope): boolean {
  return getPlan(scope).some((step) => step.status === 'doing');
}

export function addStep(scope: PlanScope, input: PlanStepInput): PlanStep[] {
  const list = getPlan(scope).slice();
  const normalized = normalizeInput(input);
  if (normalized.text && list.length < MAX_PLAN_STEPS) {
    const { dependsOn: inputDependencies, ...stable } = normalized;
    const dependsOnStepIds = dependencyIdsFromIndexes(inputDependencies, list, stable.id);
    list.push({ ...stable, status: 'todo', ...(dependsOnStepIds ? { dependsOnStepIds } : {}) });
  }
  planSetRawSteps(scope, list);
  planMarkUpdated(scope);
  planRunPersist(scope);
  return list;
}

/**
 * Mark a step (1-based) doing.
 *
 * Starting a second runnable step intentionally does NOT demote an existing
 * doing step: independent plan lanes can run in parallel.
 */
export function startStep(scope: PlanScope, index: number): PlanStep[] {
  const list = getPlan(scope).slice();
  if (!phaseAllowsExecution(getPlanLifecycle(scope))) return list;
  const i = index - 1;
  if (i >= 0 && i < list.length) list[i] = { ...list[i]!, status: 'doing' };
  planSetRawSteps(scope, list);
  planMarkUpdated(scope);
  planRunPersist(scope);
  return list;
}

/** Restore an exact local step snapshot when a post-mutation shared projection fails. */
export function restorePlanSteps(scope: PlanScope, snapshot: readonly PlanStep[]): PlanStep[] {
  planRunEnsureLoaded(scope);
  const restored = snapshot.map((step) => ({
    ...step,
    ...(step.dependsOnStepIds ? { dependsOnStepIds: [...step.dependsOnStepIds] } : {}),
    ...(step.paths ? { paths: [...step.paths] } : {}),
  }));
  planSetRawSteps(scope, restored);
  if (restored.length > 0) planDeleteClearedScope(scope);
  planMarkUpdated(scope);
  planRunPersist(scope);
  return restored;
}

/** Mark a step (1-based) done and auto-advance the next todo to doing. */
export function completeStep(scope: PlanScope, index: number): PlanStep[] {
  const list = getPlan(scope).slice();
  if (!phaseAllowsExecution(getPlanLifecycle(scope))) return list;
  const i = index - 1;
  if (i >= 0 && i < list.length) {
    list[i] = { ...list[i]!, status: 'done' };
    if (!list.some((s) => s.status === 'doing')) {
      const nextTodo = list.findIndex((s) => s.status === 'todo' && depsMet(s, list));
      if (nextTodo >= 0) list[nextTodo] = { ...list[nextTodo]!, status: 'doing' };
    }
  }
  planSetRawSteps(scope, list);
  if (list.length > 0 && list.every((step) => step.status === 'done')) {
    planSetRawLifecycle(scope, 'verifying');
  }
  planMarkUpdated(scope);
  planRunPersist(scope);
  return list;
}

/**
 * Remove a step (1-based). Dependencies are kept consistent: deps on the
 * removed step are dropped, deps pointing past it are renumbered. If the
 * removed step was the only active one, the next satisfiable todo auto-advances.
 */
export function removeStep(scope: PlanScope, index: number): PlanStep[] {
  const list = getPlan(scope).slice();
  const i = index - 1;
  if (i < 0 || i >= list.length) return list;
  const [removed] = list.splice(i, 1);
  const next = list.map((step) => {
    if (!step.dependsOnStepIds?.length || !removed) return step;
    const dependencies = step.dependsOnStepIds.filter((id) => id !== removed.id);
    const { dependsOnStepIds: _dropped, ...rest } = step;
    return dependencies.length ? { ...rest, dependsOnStepIds: dependencies } : rest;
  });
  if (phaseAllowsExecution(getPlanLifecycle(scope)) && next.length > 0 && !next.some((s) => s.status === 'doing')) {
    const nextTodo = next.findIndex((s) => s.status === 'todo' && depsMet(s, next));
    if (nextTodo >= 0) next[nextTodo] = { ...next[nextTodo]!, status: 'doing' };
  }
  planSetRawSteps(scope, next);
  planMarkUpdated(scope);
  planRunPersist(scope);
  return next;
}


