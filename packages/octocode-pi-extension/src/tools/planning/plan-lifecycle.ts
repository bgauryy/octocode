/**
 * plan-lifecycle — review-phase transitions.
 * propose → accept → start → (rollback) → executing
 */

import {
  currentRfcRevision,
  clearPlanAwarenessMappings,
  getPlan,
  getPlanReviewState,
  planApplyReviewTransition,
  planBuildTransitionError,
  planHasUnresolvedBlockers,
} from './plan-store.js';
import { cleanContractText } from './plan-normalization.js';
import type { PlanScope } from './plan-scope.js';
import { depsMet } from './plan-types.js';
import type { PlanReviewTransitionResult } from './plan-types.js';

/** Enter review and bind the displayed revision to the exact current RFC bytes. */
export function proposePlanReview(scope: PlanScope): PlanReviewTransitionResult {
  const state = getPlanReviewState(scope);
  if (state.phase !== 'draft' && state.phase !== 'in_review') {
    return planBuildTransitionError(scope, 'invalid_transition', `review.propose is not valid from ${state.phase}`);
  }
  if (planHasUnresolvedBlockers(state)) {
    return planBuildTransitionError(scope, 'unresolved_blockers', 'review.propose requires all blocking questions and comments to be resolved');
  }
  const current = currentRfcRevision(scope);
  if (!current.revision) {
    const code = current.error ?? 'rfc_unreadable';
    return planBuildTransitionError(scope, code, code === 'missing_rfc' ? 'review.propose requires a linked RFC' : 'the linked RFC could not be read');
  }
  const todoSteps = getPlan(scope).map((step) => ({ ...step, status: 'todo' as const }));
  return planApplyReviewTransition(scope, 'in_review', {
    branchSnapshotId: state.branchSnapshotId,
    generation: state.generation,
    revision: current.revision,
    blockingQuestions: state.blockingQuestions,
    comments: state.comments,
  }, todoSteps, 'review');
}

/** Accept an in-review RFC revision with an optional authorization receipt. */
export function acceptPlanReview(scope: PlanScope, displayedRevision: string, authorizationReceiptId?: string): PlanReviewTransitionResult {
  const state = getPlanReviewState(scope);
  if (state.phase !== 'in_review') {
    return planBuildTransitionError(scope, 'invalid_transition', `review.accept is not valid from ${state.phase}`);
  }
  if (planHasUnresolvedBlockers(state)) {
    return planBuildTransitionError(scope, 'unresolved_blockers', 'review.accept requires all blocking questions and comments to be resolved');
  }
  const current = currentRfcRevision(scope);
  if (!current.revision) {
    const code = current.error ?? 'rfc_unreadable';
    return planBuildTransitionError(scope, code, code === 'missing_rfc' ? 'review.accept requires a linked RFC' : 'the linked RFC could not be read');
  }
  const displayed = displayedRevision.trim();
  if (!displayed || displayed !== state.revision || displayed !== current.revision) {
    return planBuildTransitionError(scope, 'revision_changed', 'the displayed RFC revision no longer matches the canonical RFC bytes');
  }
  return planApplyReviewTransition(scope, 'accepted', {
    branchSnapshotId: state.branchSnapshotId,
    generation: state.generation,
    revision: current.revision,
    acceptedRevision: current.revision,
    ...(authorizationReceiptId?.trim() ? { acceptAuthorizationReceiptId: authorizationReceiptId.trim() } : {}),
    acceptedAt: new Date().toISOString(),
    blockingQuestions: state.blockingQuestions,
    comments: state.comments,
  }, getPlan(scope).map((step) => ({ ...step, status: 'todo' as const })));
}

/** Return an in-review or accepted RFC to draft; feedback always clears acceptance. */
export function requestPlanChanges(scope: PlanScope): PlanReviewTransitionResult {
  const state = getPlanReviewState(scope);
  if (state.phase !== 'in_review' && state.phase !== 'accepted') {
    return planBuildTransitionError(scope, 'invalid_transition', `review.request_changes is not valid from ${state.phase}`);
  }
  return planApplyReviewTransition(scope, 'draft', {
    branchSnapshotId: state.branchSnapshotId,
    generation: state.generation,
    revision: state.revision,
    blockingQuestions: state.blockingQuestions,
    comments: state.comments,
  }, getPlan(scope).map((step) => ({ ...step, status: 'todo' as const })));
}

/** Start an accepted current revision and activate exactly one dependency-ready step. */
export function startAcceptedPlan(scope: PlanScope, authorizationReceiptId: string): PlanReviewTransitionResult {
  const state = getPlanReviewState(scope);
  if (state.phase !== 'accepted') {
    return planBuildTransitionError(scope, 'invalid_transition', `implementation.start is not valid from ${state.phase}`);
  }
  if (planHasUnresolvedBlockers(state)) {
    return planBuildTransitionError(scope, 'unresolved_blockers', 'implementation.start requires all blocking questions and comments to be resolved');
  }
  const receiptId = authorizationReceiptId.trim();
  if (!receiptId) {
    return planBuildTransitionError(scope, 'authorization_required', 'implementation.start requires a valid human Start authorization receipt');
  }
  const current = currentRfcRevision(scope);
  if (!current.revision || !state.acceptedRevision || current.revision !== state.acceptedRevision) {
    // Canonical bytes changed after acceptance. Return to draft.
    planApplyReviewTransition(scope, 'draft', {
      branchSnapshotId: state.branchSnapshotId,
      generation: state.generation,
      blockingQuestions: state.blockingQuestions,
      comments: state.comments,
    }, getPlan(scope).map((step) => ({ ...step, status: 'todo' as const })));
    return planBuildTransitionError(scope, current.error ?? 'revision_changed', 'the accepted RFC revision no longer matches the canonical RFC bytes');
  }
  const steps = getPlan(scope).map((step) => step.status === 'done' ? step : { ...step, status: 'todo' as const });
  const next = steps.findIndex((step) => step.status === 'todo' && depsMet(step, steps));
  if (next < 0) return planBuildTransitionError(scope, 'no_runnable_step', 'implementation.start requires one dependency-ready step');
  steps[next] = { ...steps[next]!, status: 'doing' };
  return planApplyReviewTransition(scope, 'executing', {
    branchSnapshotId: state.branchSnapshotId,
    generation: state.generation,
    revision: state.revision,
    acceptedRevision: state.acceptedRevision,
    acceptAuthorizationReceiptId: state.acceptAuthorizationReceiptId,
    startAuthorizationReceiptId: receiptId,
    acceptedAt: state.acceptedAt,
    startedAt: new Date().toISOString(),
    blockingQuestions: state.blockingQuestions,
    comments: state.comments,
  }, steps);
}

/** Recover a failed Start attempt without losing exact-revision acceptance. */
export function rollbackAcceptedPlanStart(scope: PlanScope, reason: string): PlanReviewTransitionResult {
  const state = getPlanReviewState(scope);
  clearPlanAwarenessMappings(scope);
  if (!state.acceptedRevision) return planBuildTransitionError(scope, 'invalid_transition', reason);
  return planApplyReviewTransition(scope, 'accepted', {
    branchSnapshotId: state.branchSnapshotId,
    generation: state.generation,
    revision: state.revision,
    acceptedRevision: state.acceptedRevision,
    acceptAuthorizationReceiptId: state.acceptAuthorizationReceiptId,
    acceptedAt: state.acceptedAt,
    outcomeReason: cleanContractText(reason),
    blockingQuestions: state.blockingQuestions,
    comments: state.comments,
  }, getPlan(scope).map((step) => {
    const { awarenessTaskId: _mapping, ...rest } = step;
    return { ...rest, status: 'todo' as const };
  }), 'compensate_start_failure');
}
