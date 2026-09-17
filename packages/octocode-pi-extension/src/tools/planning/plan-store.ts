/**
 * plan-store — in-memory session state, disk persistence, scope management,
 * and all getters / setters for the plan, lifecycle, RFC, decisions, and coordination.
 *
 * This module owns the authoritative Maps. plan-lifecycle.ts and plan-executor.ts
 * call the bridge exports below to mutate state atomically.
 */

import { createHash, randomUUID } from 'node:crypto';
import fs from 'node:fs';
import { logInternalError } from '../../internal-error-log.js';
import { transitionPlan, transitionPlanTo, type PlanCommand, type PlanPhase } from '../plan-domain.js';
import {
  compareAndSwapPlanProjection,
  readPlanProjection,
  writePlanBranchSnapshot,
  type PlanBranchSnapshotV1,
} from '../session-artifacts.js';
import {
  cleanContractText,
  cleanDecision,
  dependencyIdsFromIndexes,
  MAX_PLAN_STEPS,
  normalizeInput,
} from './plan-normalization.js';
import {
  artifactContextForScope,
  releasePlanScopeBinding,
  workspaceForPlanScope,
  type PlanScope,
} from './plan-scope.js';
import {
  readCoordinationFromStored,
  readDecisionsFromStored,
  readLifecycleFromStored,
  readRfcFromStored,
  reviewMetadataFromStored,
  sanitizeStoredPlan,
  type PlanStored,
} from './plan-serialization.js';
import { PlanStateRepository } from './plan-state-repository.js';
import { depsMet } from './plan-types.js';
import type {
  CurrentRfcRevision,
  PlanCoordination,
  PlanCoordinationMode,
  PlanDecision,
  PlanReviewTransitionCode,
  PlanReviewTransitionResult,
  PlanStep,
  ReviewState,
  StepInput,
} from './plan-types.js';

export { activePlanScope, artifactContextForScope, type PlanScope } from './plan-scope.js';

const MAX_DECISIONS = 20;

const state = new PlanStateRepository();
const {
  plans,
  lifecycle: planLifecycle,
  review: planReview,
  rfc: planRfc,
  decisions: planDecisions,
  coordination: planCoordination,
  loaded,
  cleared: clearedScopes,
  turnsSinceUpdate,
  persistenceFailures,
} = state;

// ─── Internal interfaces ──────────────────────────────────────────────────────

interface PlanSnapshotMeta {
  snapshotId: string;
  generation: number;
  capturedAt: string;
}

export function phaseAllowsExecution(phase: PlanPhase): boolean {
  return phase === 'executing' || phase === 'verifying';
}

// ─── Stored-plan reading helpers ──────────────────────────────────────────────

function buildStoredPlan(scope: PlanScope, steps: PlanStep[]): PlanStored {
  const rfcPath = planRfc.get(scope);
  const decisions = planDecisions.get(scope);
  const phase = planLifecycle.get(scope) ?? 'executing';
  const review = planReview.get(scope) ?? reviewMetadataFromStored(undefined);
  return {
    version: 4,
    cleared: clearedScopes.has(scope),
    scope: scope,
    steps,
    phase,
    coordination: planCoordination.get(scope) ?? readCoordinationFromStored(undefined, scope),
    rfcPath,
    ...(review.revision ? { revision: review.revision } : {}),
    ...(review.acceptedRevision ? { acceptedRevision: review.acceptedRevision } : {}),
    ...(review.acceptAuthorizationReceiptId ? { acceptAuthorizationReceiptId: review.acceptAuthorizationReceiptId } : {}),
    ...(review.startAuthorizationReceiptId ? { startAuthorizationReceiptId: review.startAuthorizationReceiptId } : {}),
    ...(review.acceptedAt ? { acceptedAt: review.acceptedAt } : {}),
    ...(review.startedAt ? { startedAt: review.startedAt } : {}),
    ...(review.outcomeReason ? { outcomeReason: review.outcomeReason } : {}),
    ...(decisions && decisions.length ? { decisions } : {}),
    ...(review.blockingQuestions.length ? { blockingQuestions: review.blockingQuestions } : {}),
    ...(review.comments.length ? { comments: review.comments } : {}),
    branchSnapshotId: review.branchSnapshotId,
    generation: review.generation,
    updatedAt: new Date().toISOString(),
  };
}

// ─── Disk I/O ─────────────────────────────────────────────────────────────────

function readStoredFromDisk(scope: PlanScope): PlanStored | undefined {
  try {
    const ctx = artifactContextForScope(scope);
    const projection = readPlanProjection<PlanStored>(ctx);
    return projection?.state.version === 4 ? projection.state : undefined;
  } catch {
    return undefined;
  }
}

function readFromDisk(scope: PlanScope): PlanStep[] {
  return sanitizeStoredPlan(readStoredFromDisk(scope));
}

function readRfcFromDisk(scope: PlanScope): string | undefined {
  return readRfcFromStored(readStoredFromDisk(scope));
}

function readDecisionsFromDisk(scope: PlanScope): PlanDecision[] | undefined {
  return readDecisionsFromStored(readStoredFromDisk(scope));
}

function readLifecycleFromDisk(scope: PlanScope): PlanPhase | undefined {
  const stored = readStoredFromDisk(scope);
  return stored ? readLifecycleFromStored(stored) : undefined;
}

function projectStoredPlan(scope: PlanScope, stored: PlanStored, meta: PlanSnapshotMeta): void {
  try {
    const ctx = artifactContextForScope(scope);
    const current = readPlanProjection<PlanStored>(ctx);
    const snapshot: PlanBranchSnapshotV1<PlanStored> = {
      version: 1,
      sourceEntryId: meta.snapshotId,
      generation: meta.generation,
      capturedAt: meta.capturedAt,
      state: stored,
    };
    writePlanBranchSnapshot(ctx, snapshot);
    const alreadyProjected = current?.sourceEntryId === snapshot.sourceEntryId
      && current.capturedAt === snapshot.capturedAt
      && JSON.stringify(current.state) === JSON.stringify(snapshot.state);
    if (alreadyProjected) return;
    const projection: PlanBranchSnapshotV1<PlanStored> = {
      ...snapshot,
      generation: (current?.generation ?? 0) + 1,
    };
    compareAndSwapPlanProjection(ctx, current?.generation ?? null, projection);
  } catch {
    // CustomEntry/in-memory state remains authoritative; the projection is rebuildable.
  }
}

function ensureReviewMetadata(scope: PlanScope): void {
  if (!planReview.has(scope)) planReview.set(scope, reviewMetadataFromStored(undefined));
}

function ensureLoaded(scope: PlanScope): void {
  if (loaded.has(scope)) {
    ensureReviewMetadata(scope);
    return;
  }
  loaded.add(scope);
  if (plans.has(scope)) {
    ensureReviewMetadata(scope);
    return;
  }
  const stored = readStoredFromDisk(scope);
  const disk = sanitizeStoredPlan(stored);
  const isCleared = stored?.cleared;
  if (stored && !isCleared) {
    plans.set(scope, disk);
    planLifecycle.set(scope, stored ? readLifecycleFromStored(stored) : 'executing');
    planReview.set(scope, reviewMetadataFromStored(stored));
    planCoordination.set(scope, readCoordinationFromStored(stored, scope));
    const rfcPath = readRfcFromStored(stored);
    if (rfcPath) planRfc.set(scope, rfcPath);
    const decisions = readDecisionsFromStored(stored);
    if (decisions) planDecisions.set(scope, decisions);
    clearedScopes.delete(scope);
  } else if (isCleared) {
    clearedScopes.add(scope);
  }
  ensureReviewMetadata(scope);
}

// ─── Branch/fork persistence ──────────────────────────────────────────────────

export const PLAN_ENTRY_TYPE = 'octocode-plan';

type PlanEntryAppender = (
  steps: PlanStep[],
  rfcPath: string | undefined,
  decisions: PlanDecision[] | undefined,
  lifecycle: PlanPhase,
  review: ReviewState,
  coordination: PlanCoordination,
  meta: PlanSnapshotMeta,
  cleared: boolean,
) => void;

let planEntryAppender: PlanEntryAppender | null = null;

export function setPlanEntryAppender(appender: PlanEntryAppender | null): void {
  planEntryAppender = appender;
}

function nextSnapshotMeta(scope: PlanScope): PlanSnapshotMeta {
  const generation = (planReview.get(scope)?.generation ?? 0) + 1;
  return { snapshotId: `plan-${randomUUID()}`, generation, capturedAt: new Date().toISOString() };
}

function appendPlanEntry(
  scope: PlanScope,
  steps: PlanStep[],
  rfcPath: string | undefined,
  decisions: PlanDecision[] | undefined,
  lifecycle: PlanPhase,
  review: ReviewState,
  coordination: PlanCoordination,
  meta: PlanSnapshotMeta,
  cleared: boolean,
): 'appended' | 'unavailable' | 'failed' {
  if (!planEntryAppender) return 'unavailable';
  try {
    planEntryAppender(steps, rfcPath, decisions, lifecycle, review, coordination, meta, cleared);
    return 'appended';
  } catch (error) {
    persistenceFailures.set(scope, error instanceof Error ? error.message : String(error));
    logInternalError('plan-persistence', error, { operation: 'append-entry', scope });
    return 'failed';
  }
}

function markUpdated(scope: PlanScope): void {
  turnsSinceUpdate.set(scope, 0);
}

function persist(scope: PlanScope): void {
  persistenceFailures.delete(scope);
  const steps = plans.get(scope) ?? [];
  const lifecycle = planLifecycle.get(scope) ?? 'abandoned';
  const meta = nextSnapshotMeta(scope);
  const previous = planReview.get(scope) ?? reviewMetadataFromStored(undefined);
  const coordination = planCoordination.get(scope) ?? readCoordinationFromStored(undefined, scope);
  planCoordination.set(scope, coordination);
  const review: ReviewState = {
    ...getPlanReviewState(scope),
    branchSnapshotId: meta.snapshotId,
    generation: meta.generation,
  };
  const appendResult = appendPlanEntry(scope, steps, planRfc.get(scope), planDecisions.get(scope), lifecycle, review, coordination, meta, clearedScopes.has(scope));
  if (appendResult === 'appended') {
    planReview.set(scope, { ...previous, branchSnapshotId: meta.snapshotId, generation: meta.generation });
    projectStoredPlan(scope, buildStoredPlan(scope, steps), meta);
  }
}

/**
 * Adopt the newest plan snapshot found in the session branch (root→leaf).
 * Returns false — leaving current state untouched — when the branch carries no
 * snapshot at all (sessions predating this feature).
 */
export function adoptPlanFromBranch(scope: PlanScope, branchEntries: unknown[], options: { clearWhenMissing?: boolean; fork?: boolean } = {}): boolean {
  for (let i = branchEntries.length - 1; i >= 0; i -= 1) {
    const entry = branchEntries[i];
    if (!entry || typeof entry !== 'object') continue;
    const rec = entry as Record<string, unknown>;
    if (rec.type !== 'custom' || rec.customType !== PLAN_ENTRY_TYPE) continue;
    const data = rec.data && typeof rec.data === 'object' ? rec.data as Record<string, unknown> : {};
    if (data.version !== 4) continue;
    const snapshotId = typeof data.branchSnapshotId === 'string' ? data.branchSnapshotId.trim() : '';
    const entryGeneration = Number.isSafeInteger(data.generation) && Number(data.generation) > 0
      ? Number(data.generation)
      : 0;
    const entryTimestamp = typeof data.capturedAt === 'string' && Number.isFinite(Date.parse(data.capturedAt))
      ? data.capturedAt
      : '';
    if (!snapshotId || entryGeneration === 0 || !entryTimestamp) continue;
    const steps = sanitizeStoredPlan(data);
    const lifecycle = readLifecycleFromStored(data);
    const rfcPath = readRfcFromStored(data);
    const decisions = readDecisionsFromStored(data);
    const coordination = readCoordinationFromStored(data, scope);
    const explicitlyCleared = data.cleared === true;
    if (explicitlyCleared) {
      plans.delete(scope);
      planRfc.delete(scope);
      planDecisions.delete(scope);
      planCoordination.delete(scope);
      clearedScopes.add(scope);
    } else {
      plans.set(scope, steps);
      clearedScopes.delete(scope);
      if (options.fork) {
        const fresh = freshCoordination(scope);
        planCoordination.set(scope, { ...fresh, mode: coordination.mode, localReason: coordination.localReason });
        for (const step of steps) {
          step.status = 'todo';
          delete step.awarenessTaskId;
        }
      } else {
        planCoordination.set(scope, coordination);
      }
      if (rfcPath) planRfc.set(scope, rfcPath);
      else planRfc.delete(scope);
      if (decisions) planDecisions.set(scope, decisions);
      else planDecisions.delete(scope);
    }
    loaded.add(scope);
    turnsSinceUpdate.set(scope, 0);
    const adoptedLifecycle = options.fork && (lifecycle === 'executing' || lifecycle === 'verifying' || lifecycle === 'blocked' || lifecycle === 'failed')
      ? (typeof data.acceptedRevision === 'string' && data.acceptedRevision.trim() ? 'accepted' : 'draft')
      : explicitlyCleared ? 'abandoned' : lifecycle;
    planLifecycle.set(scope, adoptedLifecycle);
    const adoptedReview = reviewMetadataFromStored({ ...data, branchSnapshotId: snapshotId, generation: entryGeneration });
    if (options.fork) {
      delete adoptedReview.acceptAuthorizationReceiptId;
      delete adoptedReview.startedAt;
      delete adoptedReview.startAuthorizationReceiptId;
      delete adoptedReview.outcomeReason;
    }
    planReview.set(scope, adoptedReview);
    const stored: PlanStored = {
      ...buildStoredPlan(scope, steps),
      updatedAt: typeof data.updatedAt === 'string' ? data.updatedAt : entryTimestamp,
    };
    projectStoredPlan(scope, stored, { snapshotId, generation: entryGeneration, capturedAt: entryTimestamp });
    return true;
  }
  if (options.clearWhenMissing) {
    plans.delete(scope);
    planLifecycle.delete(scope);
    planReview.delete(scope);
    planRfc.delete(scope);
    planDecisions.delete(scope);
    planCoordination.delete(scope);
    turnsSinceUpdate.delete(scope);
    loaded.add(scope);
    clearedScopes.add(scope);
  }
  return false;
}

// ─── Test hooks ───────────────────────────────────────────────────────────────

export function readPersistedPlanForTests(scope: PlanScope): PlanStep[] {
  return readFromDisk(scope);
}

export function readPersistedRfcForTests(scope: PlanScope): string | undefined {
  return readRfcFromDisk(scope);
}

export function readPersistedDecisionsForTests(scope: PlanScope): PlanDecision[] | undefined {
  return readDecisionsFromDisk(scope);
}

export function readPersistedLifecycleForTests(scope: PlanScope): PlanPhase | undefined {
  return readLifecycleFromDisk(scope);
}

// ─── Turn tracking ────────────────────────────────────────────────────────────

export function bumpPlanTurn(scope: PlanScope): number {
  if (getPlan(scope).length === 0) return 0;
  const next = (turnsSinceUpdate.get(scope) ?? 0) + 1;
  turnsSinceUpdate.set(scope, next);
  return next;
}

export function getPlanTurnsSinceUpdate(scope: PlanScope): number {
  return turnsSinceUpdate.get(scope) ?? 0;
}

export function getPlanPersistenceFailure(scope: PlanScope): string | undefined {
  return persistenceFailures.get(scope);
}

// ─── Scope helpers ────────────────────────────────────────────────────────────

function freshCoordination(scope: PlanScope): PlanCoordination {
  return {
    mode: 'auto',
    sourcePlanKey: `pi-plan-${randomUUID()}`,
    coordinationWorkspace: workspaceForPlanScope(scope),
  };
}

// ─── Public getters ───────────────────────────────────────────────────────────

export function getPlan(scope: PlanScope): PlanStep[] {
  ensureLoaded(scope);
  return plans.get(scope) ?? [];
}

export function getPlanCoordination(scope: PlanScope): PlanCoordination {
  ensureLoaded(scope);
  const current = planCoordination.get(scope) ?? readCoordinationFromStored(undefined, scope);
  planCoordination.set(scope, current);
  return { ...current };
}

export function updatePlanCoordination(
  scope: PlanScope,
  updates: {
    mode?: PlanCoordinationMode;
    localReason?: string | null;
    coordinationWorkspace?: string;
    awarenessPlanId?: string | null;
    materializedRevision?: string | null;
  },
): PlanCoordination {
  const current = getPlanCoordination(scope);
  const mode = updates.mode ?? current.mode;
  const localReason = updates.localReason === undefined
    ? current.localReason
    : cleanContractText(updates.localReason);
  if (mode === 'local' && !localReason) throw new Error('local coordination mode requires localReason');
  const coordinationWorkspace = cleanContractText(updates.coordinationWorkspace, 2_000) ?? current.coordinationWorkspace;
  const awarenessPlanId = updates.awarenessPlanId === undefined
    ? current.awarenessPlanId
    : updates.awarenessPlanId === null ? undefined : cleanContractText(updates.awarenessPlanId, 256);
  const materializedRevision = updates.materializedRevision === undefined
    ? current.materializedRevision
    : updates.materializedRevision === null ? undefined : cleanContractText(updates.materializedRevision, 256);
  const next: PlanCoordination = {
    ...current,
    mode,
    coordinationWorkspace,
    ...(localReason ? { localReason } : {}),
    ...(awarenessPlanId ? { awarenessPlanId } : {}),
    ...(materializedRevision ? { materializedRevision } : {}),
  };
  if (mode !== 'local') delete next.localReason;
  if (!awarenessPlanId) delete next.awarenessPlanId;
  if (!materializedRevision) delete next.materializedRevision;
  planCoordination.set(scope, next);
  persist(scope);
  return { ...next };
}

export function setPlanAwarenessMappings(
  scope: PlanScope,
  mapping: { awarenessPlanId: string; taskIdsByStepId: Record<string, string>; materializedRevision?: string },
): PlanStep[] {
  const list = getPlan(scope);
  const awarenessPlanId = cleanContractText(mapping.awarenessPlanId, 256);
  if (!awarenessPlanId) throw new Error('awarenessPlanId is required');
  const taskIds = new Map(Object.entries(mapping.taskIdsByStepId).map(([stepId, taskId]) => [stepId, cleanContractText(taskId, 256)]));
  for (const step of list) {
    if (!taskIds.get(step.id)) throw new Error(`missing Awareness task mapping for step ${step.id}`);
  }
  const next = list.map((step) => ({ ...step, awarenessTaskId: taskIds.get(step.id)! }));
  plans.set(scope, next);
  const current = getPlanCoordination(scope);
  planCoordination.set(scope, {
    ...current,
    awarenessPlanId,
    ...(mapping.materializedRevision ? { materializedRevision: cleanContractText(mapping.materializedRevision, 256) } : {}),
  });
  markUpdated(scope);
  persist(scope);
  return next;
}

export function clearPlanAwarenessMappings(scope: PlanScope): PlanStep[] {
  const next = getPlan(scope).map(({ awarenessTaskId: _taskId, ...step }) => step);
  plans.set(scope, next);
  const current = getPlanCoordination(scope);
  const { awarenessPlanId: _planId, materializedRevision: _revision, ...local } = current;
  planCoordination.set(scope, local);
  markUpdated(scope);
  persist(scope);
  return next;
}

export function getPlanLifecycle(scope: PlanScope): PlanPhase {
  ensureLoaded(scope);
  return planLifecycle.get(scope) ?? 'abandoned';
}

export function setPlanLifecycle(scope: PlanScope, phase: PlanPhase, outcomeReason?: string): PlanPhase {
  ensureLoaded(scope);
  clearedScopes.delete(scope);
  const current = planLifecycle.get(scope) ?? 'abandoned';
  transitionPlanTo(current, phase);
  planLifecycle.set(scope, phase);
  const review = planReview.get(scope) ?? reviewMetadataFromStored(undefined);
  const { outcomeReason: _previousReason, ...baseReview } = review;
  planReview.set(scope, {
    ...baseReview,
    ...(outcomeReason ? { outcomeReason: cleanContractText(outcomeReason) } : {}),
  });
  markUpdated(scope);
  persist(scope);
  return phase;
}

export function finishPlanVerification(scope: PlanScope, success: boolean, reason?: string): ReviewState {
  const state = getPlanReviewState(scope);
  if (state.phase !== 'verifying') return state;
  setPlanLifecycle(scope, success ? 'complete' : 'failed', reason);
  return getPlanReviewState(scope);
}

export function getPlanReviewState(scope: PlanScope): ReviewState {
  ensureLoaded(scope);
  const metadata = planReview.get(scope)!;
  return {
    phase: getPlanLifecycle(scope),
    branchSnapshotId: metadata.branchSnapshotId,
    generation: metadata.generation,
    ...(planRfc.get(scope) ? { rfcPath: planRfc.get(scope) } : {}),
    ...(metadata.revision ? { revision: metadata.revision } : {}),
    ...(metadata.acceptedRevision ? { acceptedRevision: metadata.acceptedRevision } : {}),
    ...(metadata.acceptAuthorizationReceiptId ? { acceptAuthorizationReceiptId: metadata.acceptAuthorizationReceiptId } : {}),
    ...(metadata.startAuthorizationReceiptId ? { startAuthorizationReceiptId: metadata.startAuthorizationReceiptId } : {}),
    ...(metadata.acceptedAt ? { acceptedAt: metadata.acceptedAt } : {}),
    ...(metadata.startedAt ? { startedAt: metadata.startedAt } : {}),
    ...(metadata.outcomeReason ? { outcomeReason: metadata.outcomeReason } : {}),
    decisions: planDecisions.get(scope) ?? [],
    blockingQuestions: metadata.blockingQuestions,
    comments: metadata.comments,
  };
}

// ─── RFC association ──────────────────────────────────────────────────────────

export function getPlanRfc(scope: PlanScope): string | undefined {
  ensureLoaded(scope);
  return planRfc.get(scope);
}

export function setPlanRfc(scope: PlanScope, rfcPath: string | undefined): void {
  ensureLoaded(scope);
  if (rfcPath && rfcPath.trim()) planRfc.set(scope, rfcPath.trim());
  else planRfc.delete(scope);
  persist(scope);
}

// ─── Decision log ─────────────────────────────────────────────────────────────

export function getPlanDecisions(scope: PlanScope): PlanDecision[] {
  ensureLoaded(scope);
  return planDecisions.get(scope) ?? [];
}

export function addPlanDecision(scope: PlanScope, q: string, a: string): PlanDecision[] {
  ensureLoaded(scope);
  const cq = cleanDecision(q);
  const ca = cleanDecision(a);
  if (cq && ca) {
    const list = (planDecisions.get(scope) ?? []).slice();
    list.push({ q: cq, a: ca });
    planDecisions.set(scope, list.slice(0, MAX_DECISIONS));
    persist(scope);
  }
  return planDecisions.get(scope) ?? [];
}

export function setPlanDecisions(scope: PlanScope, decisions: PlanDecision[] | undefined): PlanDecision[] {
  ensureLoaded(scope);
  const cleaned = (decisions ?? [])
    .map((d) => ({ q: cleanDecision(d?.q ?? ''), a: cleanDecision(d?.a ?? '') }))
    .filter((d) => d.q && d.a)
    .slice(0, MAX_DECISIONS);
  if (cleaned.length) planDecisions.set(scope, cleaned);
  else planDecisions.delete(scope);
  persist(scope);
  return planDecisions.get(scope) ?? [];
}

// ─── RFC byte revision ────────────────────────────────────────────────────────

export function currentRfcRevision(scope: PlanScope): CurrentRfcRevision {
  ensureLoaded(scope);
  const rfcPath = planRfc.get(scope);
  if (!rfcPath) return { error: 'missing_rfc' };
  try {
    const bytes = fs.readFileSync(rfcPath);
    return { path: rfcPath, revision: createHash('sha256').update(bytes).digest('hex') };
  } catch {
    return { path: rfcPath, error: 'rfc_unreadable' };
  }
}

// ─── Step mutations (multi-map: stay in store) ────────────────────────────────

export function setPlan(scope: PlanScope, steps: StepInput[], lifecycle: PlanPhase = 'executing'): PlanStep[] {
  const cleaned = steps.map(normalizeInput).filter((step) => step.text).slice(0, MAX_PLAN_STEPS);
  const next: PlanStep[] = cleaned.map((step) => {
    const { dependsOn, ...stable } = step;
    const dependsOnStepIds = dependencyIdsFromIndexes(dependsOn, cleaned, step.id);
    return {
      ...stable,
      status: 'todo',
      ...(dependsOnStepIds ? { dependsOnStepIds } : {}),
    };
  });
  if (phaseAllowsExecution(lifecycle)) {
    const firstRunnable = next.findIndex((step) => depsMet(step, next));
    if (firstRunnable >= 0) next[firstRunnable] = { ...next[firstRunnable]!, status: 'doing' };
  }
  plans.set(scope, next);
  clearedScopes.delete(scope);
  planCoordination.set(scope, freshCoordination(scope));
  planLifecycle.set(scope, lifecycle);
  loaded.add(scope);
  markUpdated(scope);
  persist(scope);
  return next;
}

export function clearPlan(scope: PlanScope): void {
  plans.delete(scope);
  planLifecycle.delete(scope);
  planReview.set(scope, reviewMetadataFromStored(undefined));
  planRfc.delete(scope);
  planDecisions.delete(scope);
  planCoordination.delete(scope);
  turnsSinceUpdate.delete(scope);
  loaded.add(scope);
  clearedScopes.add(scope);
  planLifecycle.set(scope, 'abandoned');
  persist(scope);
}

/** Release all process-local state after a session has persisted its final mutation. */
export function releasePlanScope(scope: PlanScope): void {
  state.release(scope);
  releasePlanScopeBinding(scope);
}

// ─── Bridge exports for plan-lifecycle.ts ────────────────────────────────────

/** Build a failed-transition result. For use by plan-lifecycle.ts only. */
export function planBuildTransitionError(scope: PlanScope, code: PlanReviewTransitionCode, message: string): PlanReviewTransitionResult {
  return { ok: false, code, message, state: getPlanReviewState(scope), steps: getPlan(scope) };
}

/** Whether there are unresolved blocking questions/comments. For use by plan-lifecycle.ts only. */
export function planHasUnresolvedBlockers(state: ReviewState): boolean {
  return state.blockingQuestions.some((question) => question.blocking && !question.answer?.trim())
    || state.comments.some((comment) => comment.blocking && !comment.resolved);
}

/**
 * Atomically transition review state. For use by plan-lifecycle.ts only.
 * Validates the phase transition, writes all review Maps, marks updated, persists.
 */
export function planApplyReviewTransition(
  scope: PlanScope,
  phase: PlanPhase,
  metadata: Omit<ReviewState, 'phase' | 'rfcPath' | 'decisions'>,
  steps: PlanStep[] = getPlan(scope),
  command?: PlanCommand,
): PlanReviewTransitionResult {
  const current = planLifecycle.get(scope) ?? 'abandoned';
  if (command) {
    const transition = transitionPlan(current, command);
    if (transition.to !== phase) throw new Error(`Plan command ${command} does not transition to ${phase}`);
  } else {
    transitionPlanTo(current, phase);
  }
  plans.set(scope, steps);
  clearedScopes.delete(scope);
  planLifecycle.set(scope, phase);
  planReview.set(scope, metadata);
  markUpdated(scope);
  persist(scope);
  return { ok: true, state: getPlanReviewState(scope), steps: getPlan(scope) };
}

// ─── Bridge exports for plan-executor.ts ─────────────────────────────────────

/** Set the raw steps Map. For use by plan-executor.ts only. */
export function planSetRawSteps(scope: PlanScope, steps: PlanStep[]): void {
  plans.set(scope, steps);
}

/** Set the raw lifecycle Map. For use by plan-executor.ts only. */
export function planSetRawLifecycle(scope: PlanScope, phase: PlanPhase): void {
  planLifecycle.set(scope, phase);
}

/** Delete from clearedScopes. For use by plan-executor.ts only. */
export function planDeleteClearedScope(scope: PlanScope): void {
  clearedScopes.delete(scope);
}

/** Call markUpdated. For use by plan-executor.ts only. */
export function planMarkUpdated(scope: PlanScope): void {
  markUpdated(scope);
}

/** Call persist. For use by plan-executor.ts only. */
export function planRunPersist(scope: PlanScope): void {
  persist(scope);
}

/** Call ensureLoaded. For use by plan-executor.ts only. */
export function planRunEnsureLoaded(scope: PlanScope): void {
  ensureLoaded(scope);
}

