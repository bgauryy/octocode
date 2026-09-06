/**
 * active-plan — a compaction-durable, session-scoped task breakdown.
 *
 * The think-first "task breakdown gate" tells the agent to decompose non-trivial work into
 * explicit steps. Historically that breakdown lived only in the model's prose, so it was
 * lossy across compaction (plan-amnesia). This module gives it a real home:
 *   - an in-memory per-session step list (survives compaction — same process)
 *   - projected through the canonical read model every turn via `before_agent_start`,
 *     so the plan is immune to summarizer loss — exactly the
 *     mechanism proven for `<dynamic_capabilities>`.
 *
 * Pure + deterministic; the `plan` tool is a thin wrapper over these functions.
 */

import { createHash, randomUUID } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { transitionPlan, transitionPlanTo, type PlanCommand, type PlanPhase } from './plan-domain.js';
import {
  compareAndSwapPlanProjection,
  createSessionArtifactContext,
  readPlanProjection,
  writePlanBranchSnapshot,
  type PlanBranchSnapshotV1,
  type SessionIdentityInput,
} from './session-artifacts.js';

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
  /** Persisted mapping created when the plan is materialized into Awareness. */
  awarenessTaskId?: string;
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
  /** Stable origin identity used for idempotent Awareness materialization. */
  sourcePlanKey: string;
  awarenessPlanId?: string;
  coordinationWorkspace: string;
  materializedRevision?: string;
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

function cleanDeps(deps: unknown): number[] | undefined {
  if (!Array.isArray(deps)) return undefined;
  const out = deps.filter((d): d is number => Number.isInteger(d) && (d as number) >= 1).slice(0, MAX_STEPS);
  return out.length ? out : undefined;
}

function cleanStepIds(ids: unknown): string[] | undefined {
  if (!Array.isArray(ids)) return undefined;
  const out = [...new Set(ids.filter((id): id is string => typeof id === 'string' && id.trim().length > 0).map((id) => id.trim()))].slice(0, MAX_STEPS);
  return out.length ? out : undefined;
}

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

const MAX_STEPS = 40;
const MAX_STEP_CHARS = 160;
const MAX_DECISIONS = 20;
const MAX_DECISION_CHARS = 300;
const MAX_REVIEW_ITEMS = 100;
const MAX_REVIEW_TEXT_CHARS = 8_000;

// Keyed by session scope (cwd + Pi session file when available). Module-scoped
// in-memory cache; backed by disk so the plan survives compaction and process
// restart of the same session without leaking into a fresh session in the same cwd.
const plans = new Map<string, PlanStep[]>();
// Review phase is branch-authoritative; all pre-Start phases keep checklist steps non-running.
const planLifecycle = new Map<string, PlanPhase>();
const planReview = new Map<string, Omit<ReviewState, 'phase' | 'rfcPath' | 'decisions'>>();
// Plan-level RFC association (scope → absolute RFC.md path). Set once the plan
// is derived from an accepted RFC; the plan surface renders that document and
// the enforcement gate requires it for consequential work. Kept beside `plans`
// so it travels with the same persistence + branch-adoption path.
const planRfc = new Map<string, string>();
// Plan-level decision log (scope → {q,a}[]): the clarify-phase interview answers
// and any gate justifications. Durable rationale for the plan — persisted and
// branch-adopted with the same pattern as `planRfc`.
const planDecisions = new Map<string, PlanDecision[]>();
// Shared execution projection metadata. This travels with the same branch snapshot
// as steps so retries and tree navigation retain exact Awareness mappings.
const planCoordination = new Map<string, PlanCoordination>();
const loaded = new Set<string>();

export interface ActivePlanContext {
  cwd?: string;
  sessionManager?: {
    getSessionId?(): string | undefined;
    getSessionFile?(): string | undefined;
    getBranch?(): unknown[];
  };
}

interface PlanStored {
  version: 4;
  cleared: boolean;
  outcomeReason?: string;
  scope: string;
  steps: PlanStep[];
  phase?: PlanPhase;
  rfcPath?: string;
  revision?: string;
  acceptedRevision?: string;
  acceptAuthorizationReceiptId?: string;
  startAuthorizationReceiptId?: string;
  acceptedAt?: string;
  startedAt?: string;
  decisions?: PlanDecision[];
  blockingQuestions?: ReviewQuestion[];
  comments?: PlanReviewComment[];
  coordination?: PlanCoordination;
  branchSnapshotId?: string;
  generation?: number;
  updatedAt: string;
}


interface PlanSnapshotMeta {
  snapshotId: string;
  generation: number;
  capturedAt: string;
}

interface ScopeBinding {
  identityInput: SessionIdentityInput;
}

const scopeBindings = new Map<string, ScopeBinding>();
const clearedScopes = new Set<string>();

export function activePlanScope(ctx?: ActivePlanContext): string {
  const cwd = ctx?.cwd ?? process.cwd();
  const sessionId = ctx?.sessionManager?.getSessionId?.()?.trim();
  const sessionFile = ctx?.sessionManager?.getSessionFile?.()?.trim();
  const scope = sessionId
    ? `${cwd}\0id:${sessionId}`
    : sessionFile
      ? `${cwd}\0${sessionFile}`
      : cwd;
  scopeBindings.set(scope, { identityInput: { cwd, sessionManager: ctx?.sessionManager } });
  return scope;
}

function bindingForScope(scope: string): ScopeBinding {
  const known = scopeBindings.get(scope);
  if (known) return known;
  const separator = scope.indexOf('\0');
  if (separator < 0) return { identityInput: { cwd: scope } };
  const cwd = scope.slice(0, separator);
  const discriminator = scope.slice(separator + 1);
  if (discriminator.startsWith('id:')) {
    const sessionId = discriminator.slice(3);
    return { identityInput: { cwd, sessionManager: { getSessionId: () => sessionId } } };
  }
  return {
    identityInput: { cwd, sessionManager: { getSessionFile: () => discriminator } },
  };
}

export function artifactContextForScope(scope: string) {
  return createSessionArtifactContext(bindingForScope(scope).identityInput);
}

function cleanContractText(value: unknown, max = 2_000): string | undefined {
  if (typeof value !== 'string') return undefined;
  const text = value.replace(/\s+/g, ' ').trim();
  return text ? text.slice(0, max) : undefined;
}

function cleanPaths(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const paths = [...new Set(value.filter((item): item is string => typeof item === 'string').map((item) => item.trim()).filter(Boolean))].slice(0, 100);
  return paths.length ? paths : undefined;
}

function sanitizeStored(raw: unknown): PlanStep[] {
  if (!raw || typeof raw !== 'object' || !Array.isArray((raw as { steps?: unknown }).steps)) return [];
  const record = raw as Record<string, unknown>;
  if (record.version !== 4) return [];
  const sourceSteps = record.steps as unknown[];
  const out: PlanStep[] = [];
  const sourceRecords: Record<string, unknown>[] = [];
  const usedIds = new Set<string>();
  for (let sourceIndex = 0; sourceIndex < sourceSteps.length; sourceIndex += 1) {
    const candidate = sourceSteps[sourceIndex];
    if (!candidate || typeof candidate !== 'object') continue;
    const rec = candidate as Record<string, unknown>;
    const text = typeof rec.text === 'string' ? clean(rec.text) : '';
    if (!text) continue;
    const id = typeof rec.id === 'string' ? rec.id.trim().slice(0, 128) : '';
    if (!id || usedIds.has(id)) continue;
    usedIds.add(id);
    const status: StepStatus = rec.status === 'doing' || rec.status === 'done' ? rec.status : 'todo';
    const step: PlanStep = { id, text, status };
    if (typeof rec.activeForm === 'string' && rec.activeForm.trim()) step.activeForm = clean(rec.activeForm);
    const paths = cleanPaths(rec.paths);
    const reasoning = cleanContractText(rec.reasoning);
    const acceptance = cleanContractText(rec.acceptance);
    const checkCommand = cleanContractText(rec.checkCommand);
    const awarenessTaskId = cleanContractText(rec.awarenessTaskId, 256);
    if (paths) step.paths = paths;
    if (reasoning) step.reasoning = reasoning;
    if (acceptance) step.acceptance = acceptance;
    if (checkCommand) step.checkCommand = checkCommand;
    if (awarenessTaskId) step.awarenessTaskId = awarenessTaskId;
    out.push(step);
    sourceRecords.push(rec);
    if (out.length >= MAX_STEPS) break;
  }
  const knownIds = new Set(out.map((step) => step.id));
  out.forEach((step, index) => {
    const rec = sourceRecords[index]!;
    const dependencies = cleanStepIds(rec.dependsOnStepIds)?.filter((id) => id !== step.id && knownIds.has(id));
    if (dependencies?.length) step.dependsOnStepIds = dependencies;
  });
  return out;
}

function workspaceForScope(scope: string): string {
  return bindingForScope(scope).identityInput.cwd ?? scope.split('\0', 1)[0]!;
}

function freshCoordination(scope: string): PlanCoordination {
  return {
    mode: 'auto',
    sourcePlanKey: `pi-plan-${randomUUID()}`,
    coordinationWorkspace: workspaceForScope(scope),
  };
}

function readCoordinationFromStored(raw: unknown, scope: string): PlanCoordination {
  const root = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {};
  const value = root.coordination && typeof root.coordination === 'object'
    ? root.coordination as Record<string, unknown>
    : {};
  const mode: PlanCoordinationMode = value.mode === 'required' || value.mode === 'local' ? value.mode : 'auto';
  const sourcePlanKey = cleanContractText(value.sourcePlanKey, 256) ?? `pi-plan-${randomUUID()}`;
  const coordinationWorkspace = cleanContractText(value.coordinationWorkspace, 2_000) ?? workspaceForScope(scope);
  const localReason = cleanContractText(value.localReason);
  const awarenessPlanId = cleanContractText(value.awarenessPlanId, 256);
  const materializedRevision = cleanContractText(value.materializedRevision, 256);
  return {
    mode,
    sourcePlanKey,
    coordinationWorkspace,
    ...(localReason ? { localReason } : {}),
    ...(awarenessPlanId ? { awarenessPlanId } : {}),
    ...(materializedRevision ? { materializedRevision } : {}),
  };
}

/** Extract a stored rfcPath (absolute string) from a persisted/snapshot record, if present. */
function readRfcFromStored(raw: unknown): string | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const val = (raw as Record<string, unknown>).rfcPath;
  return typeof val === 'string' && val.trim() ? val : undefined;
}

/** Extract a validated decision log from a persisted/snapshot record, if present. */
function readDecisionsFromStored(raw: unknown): PlanDecision[] | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const arr = (raw as Record<string, unknown>).decisions;
  if (!Array.isArray(arr)) return undefined;
  const out: PlanDecision[] = [];
  for (const d of arr) {
    if (!d || typeof d !== 'object') continue;
    const rec = d as Record<string, unknown>;
    const q = typeof rec.q === 'string' ? cleanDecision(rec.q) : '';
    const a = typeof rec.a === 'string' ? cleanDecision(rec.a) : '';
    if (!q || !a) continue;
    out.push({ q, a });
    if (out.length >= MAX_DECISIONS) break;
  }
  return out.length ? out : undefined;
}

const PLAN_PHASE_SET = new Set<PlanPhase>([
  'researching', 'needs_answers', 'draft', 'in_review', 'accepted',
  'executing', 'verifying', 'complete', 'blocked', 'failed', 'abandoned',
]);

function readLifecycleFromStored(raw: unknown): PlanPhase {
  if (!raw || typeof raw !== 'object') return 'executing';
  const rec = raw as Record<string, unknown>;
  if (typeof rec.phase === 'string' && PLAN_PHASE_SET.has(rec.phase as PlanPhase)) return rec.phase as PlanPhase;
  return 'executing';
}

function cleanReviewText(value: unknown): string {
  return typeof value === 'string' ? value.trim().slice(0, MAX_REVIEW_TEXT_CHARS) : '';
}

function readQuestionsFromStored(raw: unknown): ReviewQuestion[] {
  const value = raw && typeof raw === 'object' ? (raw as Record<string, unknown>).blockingQuestions : undefined;
  if (!Array.isArray(value)) return [];
  return value.flatMap((item): ReviewQuestion[] => {
    if (!item || typeof item !== 'object') return [];
    const rec = item as Record<string, unknown>;
    const id = cleanReviewText(rec.id);
    const prompt = cleanReviewText(rec.prompt);
    if (!id || !prompt) return [];
    const answer = cleanReviewText(rec.answer);
    return [{ id, prompt, blocking: rec.blocking !== false, ...(answer ? { answer } : {}) }];
  }).slice(0, MAX_REVIEW_ITEMS);
}

function readCommentsFromStored(raw: unknown): PlanReviewComment[] {
  const value = raw && typeof raw === 'object' ? (raw as Record<string, unknown>).comments : undefined;
  if (!Array.isArray(value)) return [];
  return value.flatMap((item): PlanReviewComment[] => {
    if (!item || typeof item !== 'object') return [];
    const rec = item as Record<string, unknown>;
    const id = cleanReviewText(rec.id);
    const body = cleanReviewText(rec.body);
    if (!id || !body) return [];
    const section = cleanReviewText(rec.section);
    return [{ id, body, blocking: rec.blocking === true, resolved: rec.resolved === true, ...(section ? { section } : {}) }];
  }).slice(0, MAX_REVIEW_ITEMS);
}

function readOptionalTimestamp(raw: Record<string, unknown>, key: string): string | undefined {
  const value = raw[key];
  return typeof value === 'string' && Number.isFinite(Date.parse(value)) ? value : undefined;
}

function reviewMetadataFromStored(raw: unknown): Omit<ReviewState, 'phase' | 'rfcPath' | 'decisions'> {
  const rec = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {};
  const branchSnapshotId = typeof rec.branchSnapshotId === 'string' && rec.branchSnapshotId.trim()
    ? rec.branchSnapshotId
    : 'untracked-plan';
  const generation = Number.isSafeInteger(rec.generation) && Number(rec.generation) >= 0 ? Number(rec.generation) : 0;
  const revision = typeof rec.revision === 'string' && rec.revision.trim() ? rec.revision : undefined;
  const acceptedRevision = typeof rec.acceptedRevision === 'string' && rec.acceptedRevision.trim() ? rec.acceptedRevision : undefined;
  const acceptAuthorizationReceiptId = typeof rec.acceptAuthorizationReceiptId === 'string' && rec.acceptAuthorizationReceiptId.trim() ? rec.acceptAuthorizationReceiptId : undefined;
  const startAuthorizationReceiptId = typeof rec.startAuthorizationReceiptId === 'string' && rec.startAuthorizationReceiptId.trim() ? rec.startAuthorizationReceiptId : undefined;
  const outcomeReason = cleanContractText(rec.outcomeReason);
  return {
    branchSnapshotId,
    generation,
    ...(revision ? { revision } : {}),
    ...(acceptedRevision ? { acceptedRevision } : {}),
    ...(acceptAuthorizationReceiptId ? { acceptAuthorizationReceiptId } : {}),
    ...(startAuthorizationReceiptId ? { startAuthorizationReceiptId } : {}),
    ...(readOptionalTimestamp(rec, 'acceptedAt') ? { acceptedAt: readOptionalTimestamp(rec, 'acceptedAt') } : {}),
    ...(readOptionalTimestamp(rec, 'startedAt') ? { startedAt: readOptionalTimestamp(rec, 'startedAt') } : {}),
    ...(outcomeReason ? { outcomeReason } : {}),
    blockingQuestions: readQuestionsFromStored(rec),
    comments: readCommentsFromStored(rec),
  };
}

function buildStoredPlan(cwd: string, steps: PlanStep[]): PlanStored {
  const rfcPath = planRfc.get(cwd);
  const decisions = planDecisions.get(cwd);
  const phase = planLifecycle.get(cwd) ?? 'executing';
  const review = planReview.get(cwd) ?? reviewMetadataFromStored(undefined);
  return {
    version: 4,
    cleared: clearedScopes.has(cwd),
    scope: cwd,
    steps,
    phase,
    coordination: planCoordination.get(cwd) ?? readCoordinationFromStored(undefined, cwd),
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

/** Read the branch-authoritative disk projection. */
function readStoredFromDisk(cwd: string): PlanStored | undefined {
  try {
    const ctx = artifactContextForScope(cwd);
    const projection = readPlanProjection<PlanStored>(ctx);
    return projection?.state.version === 4 ? projection.state : undefined;
  } catch {
    return undefined;
  }
}

/** Read the persisted plan for a workspace. Returns [] on any error or missing state. */
function readFromDisk(cwd: string): PlanStep[] {
  return sanitizeStored(readStoredFromDisk(cwd));
}

function readRfcFromDisk(cwd: string): string | undefined {
  return readRfcFromStored(readStoredFromDisk(cwd));
}

function readDecisionsFromDisk(cwd: string): PlanDecision[] | undefined {
  return readDecisionsFromStored(readStoredFromDisk(cwd));
}

function readLifecycleFromDisk(cwd: string): PlanPhase | undefined {
  const stored = readStoredFromDisk(cwd);
  return stored ? readLifecycleFromStored(stored) : undefined;
}

/** Persist a rebuildable projection and immutable branch snapshot. Never changes in-memory authority. */
function projectStoredPlan(cwd: string, stored: PlanStored, meta: PlanSnapshotMeta): void {
  try {
    const ctx = artifactContextForScope(cwd);
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

/** Lazily hydrate the in-memory plan (and its rfcPath) from disk once per workspace per process. */
function ensureLoaded(cwd: string): void {
  if (loaded.has(cwd)) return;
  loaded.add(cwd);
  if (plans.has(cwd)) return;
  const stored = readStoredFromDisk(cwd);
  const disk = sanitizeStored(stored);
  const isCleared = stored?.cleared;
  if (stored && !isCleared) {
    plans.set(cwd, disk);
    planLifecycle.set(cwd, stored ? readLifecycleFromStored(stored) : 'executing');
    planReview.set(cwd, reviewMetadataFromStored(stored));
    planCoordination.set(cwd, readCoordinationFromStored(stored, cwd));
    const rfcPath = readRfcFromStored(stored);
    if (rfcPath) planRfc.set(cwd, rfcPath);
    const decisions = readDecisionsFromStored(stored);
    if (decisions) planDecisions.set(cwd, decisions);
    clearedScopes.delete(cwd);
  } else if (isCleared) {
    clearedScopes.add(cwd);
  }
}

// ─── Branch/fork-correct persistence (pi appendEntry pattern) ─────────────────
//
// Pi's guidance: extension state belongs in session entries so /fork and /tree
// roll it back with the conversation. Disk persistence alone is branch-blind —
// a fork keeps the forked-from plan forever. So every mutation ALSO appends an
// `octocode-plan` CustomEntry snapshot (state channel, never in LLM context)
// via the appender wired in index.ts, and session_start / session_tree re-adopt
// the last snapshot found on the current branch.

/** customType of the plan-snapshot session entries. */
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

/** Wire (or clear) the host-side appender that snapshots plans into session entries. */
export function setPlanEntryAppender(appender: PlanEntryAppender | null): void {
  planEntryAppender = appender;
}

function nextSnapshotMeta(cwd: string): PlanSnapshotMeta {
  let generation = 1;
  try {
    generation = (readPlanProjection<PlanStored>(artifactContextForScope(cwd))?.generation ?? 0) + 1;
  } catch {
    // A missing/corrupt projection cannot block the authoritative CustomEntry append.
  }
  return { snapshotId: `plan-${randomUUID()}`, generation, capturedAt: new Date().toISOString() };
}

function appendPlanEntry(
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
  } catch {
    // Never let an unrecorded mutation become restorable disk state.
    return 'failed';
  }
}

/**
 * Adopt the newest plan snapshot found in the session branch (root→leaf).
 * Returns false — leaving current state untouched — when the branch carries no
 * snapshot at all (sessions predating this feature). Adoption is
 * reconciliation, not a mutation: it never re-appends a session entry.
 */
export function adoptPlanFromBranch(cwd: string, branchEntries: unknown[], options: { clearWhenMissing?: boolean; fork?: boolean } = {}): boolean {
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
    const steps = sanitizeStored(data);
    const lifecycle = readLifecycleFromStored(data);
    const rfcPath = readRfcFromStored(data);
    const decisions = readDecisionsFromStored(data);
    const coordination = readCoordinationFromStored(data, cwd);
    const explicitlyCleared = data.cleared === true;
    if (explicitlyCleared) {
      plans.delete(cwd);
      planRfc.delete(cwd);
      planDecisions.delete(cwd);
      planCoordination.delete(cwd);
      clearedScopes.add(cwd);
    } else {
      plans.set(cwd, steps);
      clearedScopes.delete(cwd);
      if (options.fork) {
        const fresh = freshCoordination(cwd);
        planCoordination.set(cwd, { ...fresh, mode: coordination.mode, localReason: coordination.localReason });
        for (const step of steps) {
          step.status = 'todo';
          delete step.awarenessTaskId;
        }
      } else {
        planCoordination.set(cwd, coordination);
      }
      // The RFC link and decision log are part of the plan snapshot, so a
      // fork/tree jump restores (or clears) them in lockstep with the steps
      // rather than leaking a stale RFC or decisions from another branch.
      if (rfcPath) planRfc.set(cwd, rfcPath);
      else planRfc.delete(cwd);
      if (decisions) planDecisions.set(cwd, decisions);
      else planDecisions.delete(cwd);
    }
    loaded.add(cwd);
    turnsSinceUpdate.set(cwd, 0);
    const adoptedLifecycle = options.fork && (lifecycle === 'executing' || lifecycle === 'verifying' || lifecycle === 'blocked' || lifecycle === 'failed')
      ? (typeof data.acceptedRevision === 'string' && data.acceptedRevision.trim() ? 'accepted' : 'draft')
      : explicitlyCleared ? 'abandoned' : lifecycle;
    planLifecycle.set(cwd, adoptedLifecycle);
    const adoptedReview = reviewMetadataFromStored({ ...data, branchSnapshotId: snapshotId, generation: entryGeneration });
    if (options.fork) {
      delete adoptedReview.acceptAuthorizationReceiptId;
      delete adoptedReview.startedAt;
      delete adoptedReview.startAuthorizationReceiptId;
      delete adoptedReview.outcomeReason;
    }
    planReview.set(cwd, adoptedReview);
    const stored: PlanStored = {
      ...buildStoredPlan(cwd, steps),
      updatedAt: typeof data.updatedAt === 'string' ? data.updatedAt : entryTimestamp,
    };
    projectStoredPlan(cwd, stored, { snapshotId, generation: entryGeneration, capturedAt: entryTimestamp });
    return true;
  }
  if (options.clearWhenMissing) {
    plans.delete(cwd);
    planLifecycle.delete(cwd);
    planReview.delete(cwd);
    planRfc.delete(cwd);
    planDecisions.delete(cwd);
    planCoordination.delete(cwd);
    turnsSinceUpdate.delete(cwd);
    loaded.add(cwd);
    clearedScopes.add(cwd);
  }
  return false;
}

/** Persist one versioned CustomEntry and its rebuildable session-root projection. */
function persist(cwd: string): void {
  const steps = plans.get(cwd) ?? [];
  const lifecycle = planLifecycle.get(cwd) ?? 'abandoned';
  const meta = nextSnapshotMeta(cwd);
  const previous = planReview.get(cwd) ?? reviewMetadataFromStored(undefined);
  const coordination = planCoordination.get(cwd) ?? readCoordinationFromStored(undefined, cwd);
  planCoordination.set(cwd, coordination);
  const review: ReviewState = {
    ...getPlanReviewState(cwd),
    branchSnapshotId: meta.snapshotId,
    generation: meta.generation,
  };
  const appendResult = appendPlanEntry(steps, planRfc.get(cwd), planDecisions.get(cwd), lifecycle, review, coordination, meta, clearedScopes.has(cwd));
  if (appendResult === 'appended') {
    planReview.set(cwd, { ...previous, branchSnapshotId: meta.snapshotId, generation: meta.generation });
    projectStoredPlan(cwd, buildStoredPlan(cwd, steps), meta);
  }
}

/** Test hook: read the persisted plan straight from disk, bypassing the in-memory cache. */
export function readPersistedPlanForTests(cwd: string): PlanStep[] {
  return readFromDisk(cwd);
}

/** Test hook: read the persisted rfcPath straight from disk, bypassing the in-memory cache. */
export function readPersistedRfcForTests(cwd: string): string | undefined {
  return readRfcFromDisk(cwd);
}

/** Test hook: read the persisted decision log straight from disk, bypassing the in-memory cache. */
export function readPersistedDecisionsForTests(cwd: string): PlanDecision[] | undefined {
  return readDecisionsFromDisk(cwd);
}

/** Test hook: read the persisted lifecycle straight from disk. */
export function readPersistedLifecycleForTests(cwd: string): PlanPhase | undefined {
  return readLifecycleFromDisk(cwd);
}

// ─── Plan ↔ RFC association ───────────────────────────────────────────────────

/** The absolute RFC.md path this plan was derived from, if any. */
export function getPlanRfc(cwd: string): string | undefined {
  ensureLoaded(cwd);
  return planRfc.get(cwd);
}

/**
 * Associate (or clear, with `undefined`) the RFC document this plan derives from.
 * Persisted alongside the steps so the plan surface can render the RFC and the
 * enforcement gate can require it. No-op persistence when there is no plan yet —
 * the RFC link is meaningless without steps and the stepless disk file is dropped.
 */
export function setPlanRfc(cwd: string, rfcPath: string | undefined): void {
  ensureLoaded(cwd);
  if (rfcPath && rfcPath.trim()) planRfc.set(cwd, rfcPath.trim());
  else planRfc.delete(cwd);
  persist(cwd);
}

// ─── Decision log ─────────────────────────────────────────────────────────────

/** The recorded clarify-phase decisions for this plan (question → answer). */
export function getPlanDecisions(cwd: string): PlanDecision[] {
  ensureLoaded(cwd);
  return planDecisions.get(cwd) ?? [];
}

/** Append one decision (question → answer). Ignored when either side is empty. Persists. */
export function addPlanDecision(cwd: string, q: string, a: string): PlanDecision[] {
  ensureLoaded(cwd);
  const cq = cleanDecision(q);
  const ca = cleanDecision(a);
  if (cq && ca) {
    const list = (planDecisions.get(cwd) ?? []).slice();
    list.push({ q: cq, a: ca });
    planDecisions.set(cwd, list.slice(0, MAX_DECISIONS));
    persist(cwd);
  }
  return planDecisions.get(cwd) ?? [];
}

/** Replace the whole decision log (or clear with []/undefined). Persists. */
export function setPlanDecisions(cwd: string, decisions: PlanDecision[] | undefined): PlanDecision[] {
  ensureLoaded(cwd);
  const cleaned = (decisions ?? [])
    .map((d) => ({ q: cleanDecision(d?.q ?? ''), a: cleanDecision(d?.a ?? '') }))
    .filter((d) => d.q && d.a)
    .slice(0, MAX_DECISIONS);
  if (cleaned.length) planDecisions.set(cwd, cleaned);
  else planDecisions.delete(cwd);
  persist(cwd);
  return planDecisions.get(cwd) ?? [];
}

export interface RfcResolution {
  /** Absolute path to the resolved RFC.md, present only when ok. */
  path?: string;
  /** Human-readable reason the input could not be resolved, present only when not ok. */
  error?: string;
}

/**
 * Resolve a user/agent-supplied RFC reference against a workspace, enforcing that
 * it lands on an existing file inside `<workspace>/.octocode/rfc/`. Accepts either
 * a directory (→ its `RFC.md`) or a Markdown file path (absolute or relative to
 * the workspace). Guards against path traversal and symlink escape so the plan
 * surface never reads — nor the local server ever exposes — a file outside the
 * workspace's RFC tree. Pure: touches only the filesystem, never the plan maps.
 */
export function resolveRfcPath(workspace: string, input: string): RfcResolution {
  const raw = String(input ?? '').trim();
  if (!raw) return { error: 'no RFC path given' };
  const rfcRoot = path.resolve(workspace, '.octocode', 'rfc');
  try {
    let candidate = path.isAbsolute(raw) ? path.resolve(raw) : path.resolve(workspace, raw);
    let stat: fs.Stats;
    try {
      stat = fs.statSync(candidate);
    } catch {
      return { error: `no such RFC path: ${raw}` };
    }
    if (stat.isDirectory()) candidate = path.join(candidate, 'RFC.md');
    // Resolve symlinks before the containment check so a symlinked file cannot
    // point outside the RFC tree while appearing to live inside it.
    let real: string;
    try {
      real = fs.realpathSync(candidate);
    } catch {
      return { error: `no such RFC file: ${path.relative(workspace, candidate) || candidate}` };
    }
    const realRoot = fs.existsSync(rfcRoot) ? fs.realpathSync(rfcRoot) : rfcRoot;
    const withinRoot = real === realRoot || real.startsWith(realRoot + path.sep);
    if (!withinRoot) {
      return { error: `RFC must live under .octocode/rfc/ (got ${raw})` };
    }
    if (!fs.statSync(real).isFile()) {
      return { error: `RFC path is not a file: ${raw}` };
    }
    return { path: real };
  } catch (err) {
    return { error: `could not resolve RFC path: ${(err as Error).message}` };
  }
}

// Turns since the plan was last mutated, per workspace. Reset to 0 on every mutation;
// bumped once per turn from before_agent_start. Powers the stale-plan nudge.
const turnsSinceUpdate = new Map<string, number>();

/** After this many turns with no plan update, the addendum nudges the agent to update or clear it. */
export const STALE_PLAN_TURNS = 10;

function markUpdated(cwd: string): void {
  turnsSinceUpdate.set(cwd, 0);
}

/** Bump the per-turn staleness counter (called once per turn). Returns the new count. */
export function bumpPlanTurn(cwd: string): number {
  if (getPlan(cwd).length === 0) return 0;
  const next = (turnsSinceUpdate.get(cwd) ?? 0) + 1;
  turnsSinceUpdate.set(cwd, next);
  return next;
}

export function getPlanTurnsSinceUpdate(cwd: string): number {
  return turnsSinceUpdate.get(cwd) ?? 0;
}

function clean(text: string): string {
  const oneLine = String(text ?? '').replace(/\s+/g, ' ').trim();
  return oneLine.length > MAX_STEP_CHARS ? `${oneLine.slice(0, MAX_STEP_CHARS - 1)}…` : oneLine;
}

/** Collapse + cap decision text (longer budget than a step — a decision can carry a short rationale). */
function cleanDecision(text: string): string {
  const oneLine = String(text ?? '').replace(/\s+/g, ' ').trim();
  return oneLine.length > MAX_DECISION_CHARS ? `${oneLine.slice(0, MAX_DECISION_CHARS - 1)}…` : oneLine;
}

export function getPlan(cwd: string): PlanStep[] {
  ensureLoaded(cwd);
  return plans.get(cwd) ?? [];
}

export function getPlanCoordination(cwd: string): PlanCoordination {
  ensureLoaded(cwd);
  const current = planCoordination.get(cwd) ?? readCoordinationFromStored(undefined, cwd);
  planCoordination.set(cwd, current);
  return { ...current };
}

export function updatePlanCoordination(
  cwd: string,
  updates: {
    mode?: PlanCoordinationMode;
    localReason?: string | null;
    coordinationWorkspace?: string;
    awarenessPlanId?: string | null;
    materializedRevision?: string | null;
  },
): PlanCoordination {
  const current = getPlanCoordination(cwd);
  const mode = updates.mode ?? current.mode;
  const localReason = updates.localReason === undefined
    ? current.localReason
    : cleanContractText(updates.localReason);
  if (mode === 'local' && !localReason) throw new Error('local coordination mode requires localReason');
  const coordinationWorkspace = cleanContractText(updates.coordinationWorkspace, 2_000) ?? current.coordinationWorkspace;
  const awarenessPlanId = updates.awarenessPlanId === undefined
    ? current.awarenessPlanId
    : cleanContractText(updates.awarenessPlanId, 256);
  const materializedRevision = updates.materializedRevision === undefined
    ? current.materializedRevision
    : cleanContractText(updates.materializedRevision, 256);
  const next: PlanCoordination = {
    mode,
    sourcePlanKey: current.sourcePlanKey,
    coordinationWorkspace,
    ...(mode === 'local' && localReason ? { localReason } : {}),
    ...(awarenessPlanId ? { awarenessPlanId } : {}),
    ...(materializedRevision ? { materializedRevision } : {}),
  };
  planCoordination.set(cwd, next);
  if (getPlan(cwd).length > 0) persist(cwd);
  return { ...next };
}

export function setPlanAwarenessMappings(
  cwd: string,
  mapping: { awarenessPlanId: string; taskIdsByStepId: Record<string, string>; materializedRevision?: string },
): PlanStep[] {
  const list = getPlan(cwd);
  const awarenessPlanId = cleanContractText(mapping.awarenessPlanId, 256);
  if (!awarenessPlanId) throw new Error('awarenessPlanId is required');
  const taskIds = new Map(Object.entries(mapping.taskIdsByStepId).map(([stepId, taskId]) => [stepId, cleanContractText(taskId, 256)]));
  for (const step of list) {
    if (!taskIds.get(step.id)) throw new Error(`missing Awareness task mapping for step ${step.id}`);
  }
  const next = list.map((step) => ({ ...step, awarenessTaskId: taskIds.get(step.id)! }));
  plans.set(cwd, next);
  const current = getPlanCoordination(cwd);
  planCoordination.set(cwd, {
    ...current,
    awarenessPlanId,
    ...(mapping.materializedRevision ? { materializedRevision: cleanContractText(mapping.materializedRevision, 256) } : {}),
  });
  markUpdated(cwd);
  persist(cwd);
  return next;
}

export function clearPlanAwarenessMappings(cwd: string): PlanStep[] {
  const next = getPlan(cwd).map(({ awarenessTaskId: _taskId, ...step }) => step);
  plans.set(cwd, next);
  const current = getPlanCoordination(cwd);
  const { awarenessPlanId: _planId, materializedRevision: _revision, ...local } = current;
  planCoordination.set(cwd, local);
  markUpdated(cwd);
  persist(cwd);
  return next;
}

export function getPlanLifecycle(cwd: string): PlanPhase {
  ensureLoaded(cwd);
  return planLifecycle.get(cwd) ?? 'abandoned';
}

/** Persist a meaningful lifecycle even before executable steps exist. */
export function setPlanLifecycle(cwd: string, phase: PlanPhase, outcomeReason?: string): PlanPhase {
  ensureLoaded(cwd);
  clearedScopes.delete(cwd);
    const current = planLifecycle.get(cwd) ?? 'abandoned';
    transitionPlanTo(current, phase);
    planLifecycle.set(cwd, phase);
  const review = planReview.get(cwd) ?? reviewMetadataFromStored(undefined);
  const { outcomeReason: _previousReason, ...baseReview } = review;
  planReview.set(cwd, {
    ...baseReview,
    ...(outcomeReason ? { outcomeReason: cleanContractText(outcomeReason) } : {}),
  });
  markUpdated(cwd);
  persist(cwd);
  return phase;
}

export function finishPlanVerification(cwd: string, success: boolean, reason?: string): ReviewState {
  const state = getPlanReviewState(cwd);
  if (state.phase !== 'verifying') return state;
  setPlanLifecycle(cwd, success ? 'complete' : 'failed', reason);
  return getPlanReviewState(cwd);
}

export function getPlanReviewState(cwd: string): ReviewState {
  ensureLoaded(cwd);
  let metadata = planReview.get(cwd);
  if (!metadata) {
    metadata = reviewMetadataFromStored(undefined);
    planReview.set(cwd, metadata);
  }
  return {
    phase: getPlanLifecycle(cwd),
    branchSnapshotId: metadata.branchSnapshotId,
    generation: metadata.generation,
    ...(planRfc.get(cwd) ? { rfcPath: planRfc.get(cwd) } : {}),
    ...(metadata.revision ? { revision: metadata.revision } : {}),
    ...(metadata.acceptedRevision ? { acceptedRevision: metadata.acceptedRevision } : {}),
    ...(metadata.acceptAuthorizationReceiptId ? { acceptAuthorizationReceiptId: metadata.acceptAuthorizationReceiptId } : {}),
    ...(metadata.startAuthorizationReceiptId ? { startAuthorizationReceiptId: metadata.startAuthorizationReceiptId } : {}),
    ...(metadata.acceptedAt ? { acceptedAt: metadata.acceptedAt } : {}),
    ...(metadata.startedAt ? { startedAt: metadata.startedAt } : {}),
    ...(metadata.outcomeReason ? { outcomeReason: metadata.outcomeReason } : {}),
    decisions: planDecisions.get(cwd) ?? [],
    blockingQuestions: metadata.blockingQuestions,
    comments: metadata.comments,
  };
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

/** Hash the exact canonical RFC.md bytes currently linked to this plan. */
export function currentRfcRevision(cwd: string): CurrentRfcRevision {
  ensureLoaded(cwd);
  const rfcPath = planRfc.get(cwd);
  if (!rfcPath) return { error: 'missing_rfc' };
  try {
    const bytes = fs.readFileSync(rfcPath);
    return { path: rfcPath, revision: createHash('sha256').update(bytes).digest('hex') };
  } catch {
    return { path: rfcPath, error: 'rfc_unreadable' };
  }
}

function transitionError(cwd: string, code: PlanReviewTransitionCode, message: string): PlanReviewTransitionResult {
  return { ok: false, code, message, state: getPlanReviewState(cwd), steps: getPlan(cwd) };
}

function unresolvedReviewBlockers(state: ReviewState): boolean {
  return state.blockingQuestions.some((question) => question.blocking && !question.answer?.trim())
    || state.comments.some((comment) => comment.blocking && !comment.resolved);
}

function storeReviewTransition(
  cwd: string,
  phase: PlanPhase,
  metadata: Omit<ReviewState, 'phase' | 'rfcPath' | 'decisions'>,
  steps: PlanStep[] = getPlan(cwd),
  command?: PlanCommand,
): PlanReviewTransitionResult {
    const current = planLifecycle.get(cwd) ?? 'abandoned';
    if (command) {
      const transition = transitionPlan(current, command);
      if (transition.to !== phase) throw new Error(`Plan command ${command} does not transition to ${phase}`);
    } else {
      transitionPlanTo(current, phase);
    }
    plans.set(cwd, steps);
  clearedScopes.delete(cwd);
  planLifecycle.set(cwd, phase);
  planReview.set(cwd, metadata);
  markUpdated(cwd);
  persist(cwd);
  return { ok: true, state: getPlanReviewState(cwd), steps: getPlan(cwd) };
}

/** Enter review and bind the displayed revision to the exact current RFC bytes. */
export function proposePlanReview(cwd: string): PlanReviewTransitionResult {
  const state = getPlanReviewState(cwd);
  if (state.phase !== 'draft' && state.phase !== 'in_review') {
    return transitionError(cwd, 'invalid_transition', `review.propose is not valid from ${state.phase}`);
  }
  if (unresolvedReviewBlockers(state)) {
    return transitionError(cwd, 'unresolved_blockers', 'review.propose requires all blocking questions and comments to be resolved');
  }
  const current = currentRfcRevision(cwd);
  if (!current.revision) {
    const code = current.error ?? 'rfc_unreadable';
    return transitionError(cwd, code, code === 'missing_rfc' ? 'review.propose requires a linked RFC' : 'the linked RFC could not be read');
  }
  const todoSteps = getPlan(cwd).map((step) => ({ ...step, status: 'todo' as const }));
  return storeReviewTransition(cwd, 'in_review', {
    branchSnapshotId: state.branchSnapshotId,
    generation: state.generation,
    revision: current.revision,
    blockingQuestions: state.blockingQuestions,
    comments: state.comments,
  }, todoSteps);
}

/** Accept only the exact RFC revision displayed to the user; acceptance never starts work. */
export function acceptPlanReview(cwd: string, displayedRevision: string, authorizationReceiptId?: string): PlanReviewTransitionResult {
  const state = getPlanReviewState(cwd);
  if (state.phase !== 'in_review') {
    return transitionError(cwd, 'invalid_transition', `review.accept is not valid from ${state.phase}`);
  }
  if (unresolvedReviewBlockers(state)) {
    return transitionError(cwd, 'unresolved_blockers', 'review.accept requires all blocking questions and comments to be resolved');
  }
  const current = currentRfcRevision(cwd);
  if (!current.revision) {
    const code = current.error ?? 'rfc_unreadable';
    return transitionError(cwd, code, code === 'missing_rfc' ? 'review.accept requires a linked RFC' : 'the linked RFC could not be read');
  }
  const displayed = displayedRevision.trim();
  if (!displayed || displayed !== state.revision || displayed !== current.revision) {
    return transitionError(cwd, 'revision_changed', 'the displayed RFC revision no longer matches the canonical RFC bytes');
  }
  return storeReviewTransition(cwd, 'accepted', {
    branchSnapshotId: state.branchSnapshotId,
    generation: state.generation,
    revision: current.revision,
    acceptedRevision: current.revision,
    ...(authorizationReceiptId?.trim() ? { acceptAuthorizationReceiptId: authorizationReceiptId.trim() } : {}),
    acceptedAt: new Date().toISOString(),
    blockingQuestions: state.blockingQuestions,
    comments: state.comments,
  }, getPlan(cwd).map((step) => ({ ...step, status: 'todo' as const })));
}

/** Return an in-review or accepted RFC to draft; feedback always clears acceptance. */
export function requestPlanChanges(cwd: string): PlanReviewTransitionResult {
  const state = getPlanReviewState(cwd);
  if (state.phase !== 'in_review' && state.phase !== 'accepted') {
    return transitionError(cwd, 'invalid_transition', `review.request_changes is not valid from ${state.phase}`);
  }
  return storeReviewTransition(cwd, 'draft', {
    branchSnapshotId: state.branchSnapshotId,
    generation: state.generation,
    revision: state.revision,
    blockingQuestions: state.blockingQuestions,
    comments: state.comments,
  }, getPlan(cwd).map((step) => ({ ...step, status: 'todo' as const })));
}

/** Start an accepted current revision and activate exactly one dependency-ready step. */
export function startAcceptedPlan(cwd: string, authorizationReceiptId: string): PlanReviewTransitionResult {
  const state = getPlanReviewState(cwd);
  if (state.phase !== 'accepted') {
    return transitionError(cwd, 'invalid_transition', `implementation.start is not valid from ${state.phase}`);
  }
  if (unresolvedReviewBlockers(state)) {
    return transitionError(cwd, 'unresolved_blockers', 'implementation.start requires all blocking questions and comments to be resolved');
  }
  const receiptId = authorizationReceiptId.trim();
  if (!receiptId) {
    return transitionError(cwd, 'authorization_required', 'implementation.start requires a valid human Start authorization receipt');
  }
  const current = currentRfcRevision(cwd);
  if (!current.revision || !state.acceptedRevision || current.revision !== state.acceptedRevision) {
    // Canonical bytes changed after acceptance. Invalidate the sidecar acceptance
    // and return to draft; the changed bytes must be proposed and reviewed anew.
    storeReviewTransition(cwd, 'draft', {
      branchSnapshotId: state.branchSnapshotId,
      generation: state.generation,
      blockingQuestions: state.blockingQuestions,
      comments: state.comments,
    }, getPlan(cwd).map((step) => ({ ...step, status: 'todo' as const })));
    return transitionError(cwd, current.error ?? 'revision_changed', 'the accepted RFC revision no longer matches the canonical RFC bytes');
  }
  const steps = getPlan(cwd).map((step) => step.status === 'done' ? step : { ...step, status: 'todo' as const });
  const next = steps.findIndex((step) => step.status === 'todo' && depsMet(step, steps));
  if (next < 0) return transitionError(cwd, 'no_runnable_step', 'implementation.start requires one dependency-ready step');
  steps[next] = { ...steps[next]!, status: 'doing' };
  return storeReviewTransition(cwd, 'executing', {
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
export function rollbackAcceptedPlanStart(cwd: string, reason: string): PlanReviewTransitionResult {
  const state = getPlanReviewState(cwd);
  clearPlanAwarenessMappings(cwd);
  if (!state.acceptedRevision) return transitionError(cwd, 'invalid_transition', reason);
  return storeReviewTransition(cwd, 'accepted', {
    branchSnapshotId: state.branchSnapshotId,
    generation: state.generation,
    revision: state.revision,
    acceptedRevision: state.acceptedRevision,
    acceptAuthorizationReceiptId: state.acceptAuthorizationReceiptId,
    acceptedAt: state.acceptedAt,
    outcomeReason: cleanContractText(reason),
    blockingQuestions: state.blockingQuestions,
    comments: state.comments,
  }, getPlan(cwd).map((step) => {
    const { awarenessTaskId: _mapping, ...rest } = step;
    return { ...rest, status: 'todo' as const };
  }), 'compensate_start_failure');
}

function phaseAllowsExecution(phase: PlanPhase): boolean {
  return phase === 'executing' || phase === 'verifying';
}

/** Promote an accepted draft and start its first runnable step. */
export function activatePlan(cwd: string): PlanStep[] {
  const list = getPlan(cwd).slice();
  if (list.length > 0 && !list.some((step) => step.status === 'doing')) {
    const next = list.findIndex((step) => step.status === 'todo' && depsMet(step, list));
    if (next >= 0) list[next] = { ...list[next]!, status: 'doing' };
  }
  plans.set(cwd, list);
  planLifecycle.set(cwd, 'executing');
  markUpdated(cwd);
  persist(cwd);
  return list;
}

/**
 * Whether the scope has an actively owned in-progress step. Auto-compaction uses
 * this stricter signal so stale todo/blocked plan state after a finished turn
 * cannot trigger a surprise compaction; it should fire only while work is live.
 */
export function hasActivePlanWork(cwd: string): boolean {
  return getPlan(cwd).some((step) => step.status === 'doing');
}

type NormalizedStepInput = Omit<PlanStep, 'status' | 'dependsOnStepIds' | 'awarenessTaskId'> & { dependsOn?: number[] };

function normalizeInput(step: StepInput): NormalizedStepInput {
  if (typeof step === 'string') return { id: `step-${randomUUID()}`, text: clean(step) };
  const text = clean(step.text);
  const activeForm = step.activeForm ? clean(step.activeForm) : undefined;
  const dependsOn = cleanDeps(step.dependsOn);
  const paths = cleanPaths(step.paths);
  const reasoning = cleanContractText(step.reasoning);
  const acceptance = cleanContractText(step.acceptance);
  const checkCommand = cleanContractText(step.checkCommand);
  return {
    id: `step-${randomUUID()}`,
    text,
    ...(activeForm ? { activeForm } : {}),
    ...(dependsOn ? { dependsOn } : {}),
    ...(paths ? { paths } : {}),
    ...(reasoning ? { reasoning } : {}),
    ...(acceptance ? { acceptance } : {}),
    ...(checkCommand ? { checkCommand } : {}),
  };
}

function dependencyIdsFromIndexes(indexes: number[] | undefined, list: Array<{ id: string }>, ownId: string): string[] | undefined {
  if (!indexes?.length) return undefined;
  const ids = [...new Set(indexes.flatMap((index) => {
    const dependency = list[index - 1];
    return dependency && dependency.id !== ownId ? [dependency.id] : [];
  }))];
  return ids.length ? ids : undefined;
}

/** Replace the whole plan; pre-Start phases remain entirely todo. */
export function setPlan(cwd: string, steps: StepInput[], lifecycle: PlanPhase = 'executing'): PlanStep[] {
  const cleaned = steps.map(normalizeInput).filter((step) => step.text).slice(0, MAX_STEPS);
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
  plans.set(cwd, next);
  clearedScopes.delete(cwd);
  planCoordination.set(cwd, freshCoordination(cwd));
  planLifecycle.set(cwd, lifecycle);
  loaded.add(cwd);
  markUpdated(cwd);
  persist(cwd);
  return next;
}

export function addStep(cwd: string, input: PlanStepInput): PlanStep[] {
  const list = getPlan(cwd).slice();
  const normalized = normalizeInput(input);
  if (normalized.text && list.length < MAX_STEPS) {
    const { dependsOn: inputDependencies, ...stable } = normalized;
    const dependsOnStepIds = dependencyIdsFromIndexes(inputDependencies, list, stable.id);
    list.push({ ...stable, status: 'todo', ...(dependsOnStepIds ? { dependsOnStepIds } : {}) });
  }
  plans.set(cwd, list);
  markUpdated(cwd);
  persist(cwd);
  return list;
}

/**
 * Mark a step (1-based) doing.
 *
 * Starting a second runnable step intentionally does NOT demote an existing
 * doing step: independent plan lanes can run in parallel (for example, a
 * parent edit plus a read-only subagent verification lane). Callers that need
 * serial execution should complete the active step before starting the next one.
 */
export function startStep(cwd: string, index: number): PlanStep[] {
  const list = getPlan(cwd).slice();
  if (!phaseAllowsExecution(getPlanLifecycle(cwd))) return list;
  const i = index - 1;
  if (i >= 0 && i < list.length) list[i] = { ...list[i]!, status: 'doing' };
  plans.set(cwd, list);
  markUpdated(cwd);
  persist(cwd);
  return list;
}

/** Restore an exact local step snapshot when a post-mutation shared projection fails. */
export function restorePlanSteps(cwd: string, snapshot: readonly PlanStep[]): PlanStep[] {
  ensureLoaded(cwd);
  const restored = snapshot.map((step) => ({
    ...step,
    ...(step.dependsOnStepIds ? { dependsOnStepIds: [...step.dependsOnStepIds] } : {}),
    ...(step.paths ? { paths: [...step.paths] } : {}),
  }));
  plans.set(cwd, restored);
  if (restored.length > 0) clearedScopes.delete(cwd);
  markUpdated(cwd);
  persist(cwd);
  return restored;
}

/** Mark a step (1-based) done and auto-advance the next todo to doing. */
export function completeStep(cwd: string, index: number): PlanStep[] {
  const list = getPlan(cwd).slice();
  if (!phaseAllowsExecution(getPlanLifecycle(cwd))) return list;
  const i = index - 1;
  if (i >= 0 && i < list.length) {
    list[i] = { ...list[i]!, status: 'done' };
    if (!list.some((s) => s.status === 'doing')) {
      const nextTodo = list.findIndex((s) => s.status === 'todo' && depsMet(s, list));
      if (nextTodo >= 0) list[nextTodo] = { ...list[nextTodo]!, status: 'doing' };
    }
  }
  plans.set(cwd, list);
  if (list.length > 0 && list.every((step) => step.status === 'done')) {
    planLifecycle.set(cwd, 'verifying');
  }
  markUpdated(cwd);
  persist(cwd);
  return list;
}

/**
 * Remove a step (1-based). Dependencies are kept consistent: deps on the
 * removed step are dropped, deps pointing past it are renumbered. If the
 * removed step was the only active one, the next satisfiable todo auto-advances so
 * the plan always has active work while unfinished steps remain.
 */
export function removeStep(cwd: string, index: number): PlanStep[] {
  const list = getPlan(cwd).slice();
  const i = index - 1;
  if (i < 0 || i >= list.length) return list;
  const [removed] = list.splice(i, 1);
  const next = list.map((step) => {
    if (!step.dependsOnStepIds?.length || !removed) return step;
    const dependencies = step.dependsOnStepIds.filter((id) => id !== removed.id);
    const { dependsOnStepIds: _dropped, ...rest } = step;
    return dependencies.length ? { ...rest, dependsOnStepIds: dependencies } : rest;
  });
  if (phaseAllowsExecution(getPlanLifecycle(cwd)) && next.length > 0 && !next.some((s) => s.status === 'doing')) {
    const nextTodo = next.findIndex((s) => s.status === 'todo' && depsMet(s, next));
    if (nextTodo >= 0) next[nextTodo] = { ...next[nextTodo]!, status: 'doing' };
  }
  plans.set(cwd, next);
  markUpdated(cwd);
  persist(cwd);
  return next;
}

export function clearPlan(cwd: string): void {
  plans.delete(cwd);
  planLifecycle.delete(cwd);
  planReview.delete(cwd);
  planRfc.delete(cwd);
  planDecisions.delete(cwd);
  planCoordination.delete(cwd);
  turnsSinceUpdate.delete(cwd);
  loaded.add(cwd);
  clearedScopes.add(cwd);
  planLifecycle.set(cwd, 'abandoned');
  // Snapshot the cleared state too: a fork taken after clear must start clean.
  persist(cwd);
}

export const MARK: Record<StepStatus, string> = { todo: '[ ]', doing: '[~]', done: '[x]' };

/** Label for a step: the present-continuous activeForm while it is running, else the imperative text. */
export function stepLabel(s: PlanStep): string {
  return s.status === 'doing' && s.activeForm ? s.activeForm : s.text;
}
