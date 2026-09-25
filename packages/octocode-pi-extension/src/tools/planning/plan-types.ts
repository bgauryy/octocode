/**
 * plan-types — shared types, interfaces, and pure computation helpers.
 * No Map access, no I/O, no side effects.
 */

import type { PlanPhase } from '../plan-domain.js';

export type StepStatus = 'todo' | 'doing' | 'done';

export interface ReviewQuestion {
  id: string;
  prompt: string;
  answer?: string;
  blocking: boolean;
}

export interface PlanReviewComment {
  id: string;
  body: string;
  section?: string;
  blocking: boolean;
  resolved: boolean;
}

export interface ReviewState {
  phase: PlanPhase;
  branchSnapshotId: string;
  generation: number;
  rfcPath?: string;
  revision?: string;
  acceptedRevision?: string;
  acceptAuthorizationReceiptId?: string;
  startAuthorizationReceiptId?: string;
  acceptedAt?: string;
  startedAt?: string;
  outcomeReason?: string;
  decisions: PlanDecision[];
  blockingQuestions: ReviewQuestion[];
  comments: PlanReviewComment[];
}

export interface PlanStep {
  /** Stable local identity; preserved across add/remove/reorder/reload. */
  id: string;
  text: string;
  status: StepStatus;
  /** Present-continuous label shown while this step is the active one (e.g. "Editing file"). */
  activeForm?: string;
  /** Stable IDs of steps that must be `done` before this one can start. */
  dependsOnStepIds?: string[];
  /** Shared-task execution and verification contract. */
  paths?: string[];
  reasoning?: string;
  acceptance?: string;
  checkCommand?: string;
}

export interface PlanStepInput {
  text: string;
  activeForm?: string;
  /** 1-based display indices accepted only at the mutation boundary. */
  dependsOn?: number[];
  paths?: string[];
  reasoning?: string;
  acceptance?: string;
  checkCommand?: string;
}

/** A step input: either a bare imperative string, or a task-contract object. */
export type StepInput = string | PlanStepInput;

export type PlanCoordinationMode = 'auto' | 'required' | 'local';

export interface PlanCoordination {
  mode: PlanCoordinationMode;
  localReason?: string;
  /** Stable Pi session plan identity. */
  sourcePlanKey: string;
  coordinationWorkspace: string;
}

/** A recorded planning decision — the question asked in the clarify phase and the answer chosen. */
export interface PlanDecision {
  /** The question / choice point. */
  q: string;
  /** The resolved answer (chosen option label, or the free-text reply). */
  a: string;
}

/** Derived display status: a todo step whose dependencies aren't all done shows as 'blocked'. */
export type DisplayStatus = StepStatus | 'blocked';

export interface ActivePlanContext {
  cwd?: string;
  sessionManager?: {
    getSessionId?(): string | undefined;
    getSessionFile?(): string | undefined;
    getBranch?(): unknown[];
  };
}

export interface RfcResolution {
  /** Absolute path to the resolved RFC.md, present only when ok. */
  path?: string;
  /** Human-readable reason the input could not be resolved, present only when not ok. */
  error?: string;
}

export type PlanReviewTransitionCode =
  | 'invalid_transition'
  | 'missing_rfc'
  | 'rfc_unreadable'
  | 'revision_changed'
  | 'authorization_required'
  | 'unresolved_blockers'
  | 'no_runnable_step';

export interface CurrentRfcRevision {
  path?: string;
  revision?: string;
  error?: Extract<PlanReviewTransitionCode, 'missing_rfc' | 'rfc_unreadable'>;
}

export type PlanReviewTransitionResult =
  | { ok: true; state: ReviewState; steps: PlanStep[] }
  | { ok: false; code: PlanReviewTransitionCode; message: string; state: ReviewState; steps: PlanStep[] };

/** Current 1-based display indices for a step's stable dependency IDs. */
export function dependencyIndexes(step: PlanStep, list: PlanStep[]): number[] {
  if (!step.dependsOnStepIds?.length) return [];
  const indexById = new Map(list.map((candidate, index) => [candidate.id, index + 1]));
  return step.dependsOnStepIds.flatMap((id) => {
    const index = indexById.get(id);
    return index === undefined ? [] : [index];
  });
}

/** Whether all of a step's stable dependencies resolve to `done` steps. */
export function depsMet(step: PlanStep, list: PlanStep[]): boolean {
  if (!step.dependsOnStepIds?.length) return true;
  const byId = new Map(list.map((candidate) => [candidate.id, candidate]));
  return step.dependsOnStepIds.every((id) => byId.get(id)?.status === 'done');
}

/** Display status for a step: 'blocked' when it's a todo with unmet dependencies. */
export function displayStatus(step: PlanStep, list: PlanStep[]): DisplayStatus {
  return step.status === 'todo' && !depsMet(step, list) ? 'blocked' : step.status;
}

/** The planning flow phases, in order — shared by the panel stepper and the browser timeline. */
export const PLAN_PHASES = ['Research', 'Clarify', 'Draft', 'Review', 'Start', 'Work', 'Verify', 'Complete'] as const;

/** Which durable lifecycle stage is current. Returns an index into PLAN_PHASES. */
export function planPhaseIndex(phase: PlanPhase): number {
  const index: Record<PlanPhase, number> = {
    researching: 0,
    needs_answers: 1,
    draft: 2,
    in_review: 3,
    accepted: 4,
    executing: 5,
    verifying: 6,
    complete: 7,
    blocked: 5,
    failed: 5,
    abandoned: 0,
  };
  return index[phase];
}

export const MARK: Record<StepStatus, string> = { todo: '[ ]', doing: '[~]', done: '[x]' };

/** Label for a step: the present-continuous activeForm while it is running, else the imperative text. */
export function stepLabel(s: PlanStep): string {
  return s.status === 'doing' && s.activeForm ? s.activeForm : s.text;
}
