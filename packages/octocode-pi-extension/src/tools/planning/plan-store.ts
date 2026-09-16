/**
 * plan-store — in-memory session state, disk persistence, scope management,
 * and all getters / setters for the plan, lifecycle, RFC, decisions, and coordination.
 *
 * This module owns the authoritative Maps. plan-lifecycle.ts and plan-executor.ts
 * call the bridge exports below to mutate state atomically.
 */

import { createHash, randomUUID } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { transitionPlan, transitionPlanTo, type PlanCommand, type PlanPhase } from '../plan-domain.js';
import {
  compareAndSwapPlanProjection,
  createSessionArtifactContext,
  readPlanProjection,
  writePlanBranchSnapshot,
  type PlanBranchSnapshotV1,
  type SessionIdentityInput,
} from '../session-artifacts.js';
import { depsMet } from './plan-types.js';
import type {
  ActivePlanContext,
  CurrentRfcRevision,
  PlanCoordination,
  PlanCoordinationMode,
  PlanDecision,
  PlanReviewComment,
  PlanReviewTransitionCode,
  PlanReviewTransitionResult,
  PlanStep,
  RfcResolution,
  ReviewQuestion,
  ReviewState,
  StepInput,
} from './plan-types.js';

// ─── Constants ────────────────────────────────────────────────────────────────

const MAX_STEPS = 40;
const MAX_STEP_CHARS = 160;
const MAX_DECISIONS = 20;
const MAX_DECISION_CHARS = 300;
const MAX_REVIEW_ITEMS = 100;
const MAX_REVIEW_TEXT_CHARS = 8_000;

// ─── In-memory Maps ───────────────────────────────────────────────────────────

/** Keyed by session scope (cwd + Pi session file when available). */
const plans = new Map<string, PlanStep[]>();
const planLifecycle = new Map<string, PlanPhase>();
const planReview = new Map<string, Omit<ReviewState, 'phase' | 'rfcPath' | 'decisions'>>();
const planRfc = new Map<string, string>();
const planDecisions = new Map<string, PlanDecision[]>();
const planCoordination = new Map<string, PlanCoordination>();
const loaded = new Set<string>();

// ─── Internal interfaces ──────────────────────────────────────────────────────

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

// ─── Scope management ─────────────────────────────────────────────────────────

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

// ─── Private helpers ──────────────────────────────────────────────────────────

export function cleanContractText(value: unknown, max = 2_000): string | undefined {
  if (typeof value !== 'string') return undefined;
  const text = value.replace(/\s+/g, ' ').trim();
  return text ? text.slice(0, max) : undefined;
}

function cleanPaths(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const out = [...new Set(value.filter((item): item is string => typeof item === 'string').map((item) => item.trim()).filter(Boolean))].slice(0, 100);
  return out.length ? out : undefined;
}

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

function clean(text: string): string {
  const oneLine = String(text ?? '').replace(/\s+/g, ' ').trim();
  return oneLine.length > MAX_STEP_CHARS ? `${oneLine.slice(0, MAX_STEP_CHARS - 1)}…` : oneLine;
}

function cleanDecision(text: string): string {
  const oneLine = String(text ?? '').replace(/\s+/g, ' ').trim();
  return oneLine.length > MAX_DECISION_CHARS ? `${oneLine.slice(0, MAX_DECISION_CHARS - 1)}…` : oneLine;
}

function cleanReviewText(value: unknown): string {
  return typeof value === 'string' ? value.trim().slice(0, MAX_REVIEW_TEXT_CHARS) : '';
}

function readOptionalTimestamp(rec: Record<string, unknown>, key: string): string | undefined {
  const val = rec[key];
  return typeof val === 'string' && Number.isFinite(Date.parse(val)) ? val : undefined;
}

// ─── NormalizeInput (exported for plan-executor.ts) ──────────────────────────

type NormalizedStepInput = Omit<PlanStep, 'status' | 'dependsOnStepIds' | 'awarenessTaskId'> & { dependsOn?: number[] };

export function normalizeInput(step: StepInput): NormalizedStepInput {
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

export function dependencyIdsFromIndexes(indexes: number[] | undefined, list: Array<{ id: string }>, ownId: string): string[] | undefined {
  if (!indexes?.length) return undefined;
  const ids = [...new Set(indexes.flatMap((index) => {
    const dependency = list[index - 1];
    return dependency && dependency.id !== ownId ? [dependency.id] : [];
  }))];
  return ids.length ? ids : undefined;
}

export function phaseAllowsExecution(phase: PlanPhase): boolean {
  return phase === 'executing' || phase === 'verifying';
}

// ─── Stored-plan reading helpers ──────────────────────────────────────────────

function sanitizeStored(raw: unknown): PlanStep[] {
  if (!raw || typeof raw !== 'object' || !Array.isArray((raw as { steps?: unknown }).steps)) return [];
  const record = raw as Record<string, unknown>;
  if (record.version !== 4) return [];
  const sourceSteps = record.steps as unknown[];
  const out: PlanStep[] = [];
  for (const item of sourceSteps) {
    if (!item || typeof item !== 'object') continue;
    const rec = item as Record<string, unknown>;
    const id = cleanContractText(rec.id, 256);
    const text = cleanContractText(rec.text);
    if (!id || !text) continue;
    const status = rec.status === 'todo' || rec.status === 'doing' || rec.status === 'done' ? rec.status : 'todo';
    const activeForm = cleanContractText(rec.activeForm);
    const dependsOnStepIds = cleanStepIds(rec.dependsOnStepIds);
    const paths = cleanPaths(rec.paths);
    const reasoning = cleanContractText(rec.reasoning);
    const acceptance = cleanContractText(rec.acceptance);
    const checkCommand = cleanContractText(rec.checkCommand);
    const awarenessTaskId = cleanContractText(rec.awarenessTaskId, 256);
    out.push({
      id,
      text,
      status,
      ...(activeForm ? { activeForm } : {}),
      ...(dependsOnStepIds ? { dependsOnStepIds } : {}),
      ...(paths ? { paths } : {}),
      ...(reasoning ? { reasoning } : {}),
      ...(acceptance ? { acceptance } : {}),
      ...(checkCommand ? { checkCommand } : {}),
      ...(awarenessTaskId ? { awarenessTaskId } : {}),
    });
    if (out.length >= MAX_STEPS) break;
  }
  return out;
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
    return [{ id, body, blocking: rec.blocking !== false, resolved: rec.resolved === true, ...(section ? { section } : {}) }];
  }).slice(0, MAX_REVIEW_ITEMS);
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

function readRfcFromStored(raw: unknown): string | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const val = (raw as Record<string, unknown>).rfcPath;
  return typeof val === 'string' && val.trim() ? val : undefined;
}

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

function reviewMetadataFromStored(raw: unknown): Omit<ReviewState, 'phase' | 'rfcPath' | 'decisions'> {
  if (!raw || typeof raw !== 'object') {
    return { branchSnapshotId: `plan-${randomUUID()}`, generation: 0, blockingQuestions: [], comments: [] };
  }
  const rec = raw as Record<string, unknown>;
  const branchSnapshotId = typeof rec.branchSnapshotId === 'string' && rec.branchSnapshotId.trim()
    ? rec.branchSnapshotId
    : `plan-${randomUUID()}`;
  const generation = Number.isSafeInteger(rec.generation) && Number(rec.generation) >= 0 ? Number(rec.generation) : 0;
  const revision = cleanContractText(rec.revision, 256);
  const acceptedRevision = cleanContractText(rec.acceptedRevision, 256);
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

// ─── Disk I/O ─────────────────────────────────────────────────────────────────

function readStoredFromDisk(cwd: string): PlanStored | undefined {
  try {
    const ctx = artifactContextForScope(cwd);
    const projection = readPlanProjection<PlanStored>(ctx);
    return projection?.state.version === 4 ? projection.state : undefined;
  } catch {
    return undefined;
  }
}

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
    return 'failed';
  }
}

function markUpdated(cwd: string): void {
  turnsSinceUpdate.set(cwd, 0);
}

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

/**
 * Adopt the newest plan snapshot found in the session branch (root→leaf).
 * Returns false — leaving current state untouched — when the branch carries no
 * snapshot at all (sessions predating this feature).
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

// ─── Test hooks ───────────────────────────────────────────────────────────────

export function readPersistedPlanForTests(cwd: string): PlanStep[] {
  return readFromDisk(cwd);
}

export function readPersistedRfcForTests(cwd: string): string | undefined {
  return readRfcFromDisk(cwd);
}

export function readPersistedDecisionsForTests(cwd: string): PlanDecision[] | undefined {
  return readDecisionsFromDisk(cwd);
}

export function readPersistedLifecycleForTests(cwd: string): PlanPhase | undefined {
  return readLifecycleFromDisk(cwd);
}

// ─── Turn tracking ────────────────────────────────────────────────────────────

const turnsSinceUpdate = new Map<string, number>();

export function bumpPlanTurn(cwd: string): number {
  if (getPlan(cwd).length === 0) return 0;
  const next = (turnsSinceUpdate.get(cwd) ?? 0) + 1;
  turnsSinceUpdate.set(cwd, next);
  return next;
}

export function getPlanTurnsSinceUpdate(cwd: string): number {
  return turnsSinceUpdate.get(cwd) ?? 0;
}

// ─── Scope helpers ────────────────────────────────────────────────────────────

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

// ─── Public getters ───────────────────────────────────────────────────────────

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
  planCoordination.set(cwd, next);
  persist(cwd);
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

// ─── RFC association ──────────────────────────────────────────────────────────

export function getPlanRfc(cwd: string): string | undefined {
  ensureLoaded(cwd);
  return planRfc.get(cwd);
}

export function setPlanRfc(cwd: string, rfcPath: string | undefined): void {
  ensureLoaded(cwd);
  if (rfcPath && rfcPath.trim()) planRfc.set(cwd, rfcPath.trim());
  else planRfc.delete(cwd);
  persist(cwd);
}

// ─── Decision log ─────────────────────────────────────────────────────────────

export function getPlanDecisions(cwd: string): PlanDecision[] {
  ensureLoaded(cwd);
  return planDecisions.get(cwd) ?? [];
}

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

// ─── RFC path resolution ──────────────────────────────────────────────────────

export function resolveRfcPath(workspace: string, input: string): RfcResolution {
  const raw = String(input ?? '').trim();
  if (!raw) return { error: 'no RFC path given' };
  try {
    let candidate = path.isAbsolute(raw) ? path.resolve(raw) : path.resolve(workspace, raw);

    // Path must exist.
    let stat: fs.Stats;
    try {
      stat = fs.statSync(candidate);
    } catch {
      return { error: `no such RFC path: ${raw}` };
    }

    if (stat.isDirectory()) {
      // Prefer RFC.md; fall back to any .md file in the directory.
      const rfcMd = path.join(candidate, 'RFC.md');
      if (fs.existsSync(rfcMd)) {
        candidate = rfcMd;
      } else {
        let entries: string[] = [];
        try { entries = fs.readdirSync(candidate); } catch { /* ignore */ }
        const mdFile = entries.filter((f) => f.toLowerCase().endsWith('.md')).sort()[0];
        if (mdFile) {
          candidate = path.join(candidate, mdFile);
        } else {
          return { error: `RFC directory has no .md file: ${path.relative(workspace, candidate) || candidate} — add a document and retry` };
        }
      }
      try { stat = fs.statSync(candidate); } catch {
        return { error: `RFC directory candidate not readable: ${path.relative(workspace, candidate) || candidate}` };
      }
    }

    // Resolve symlinks; use realpathSync on both sides so macOS /var → /private/var compares correctly.
    let real: string;
    try {
      real = fs.realpathSync(candidate);
    } catch {
      real = candidate;
    }
    let resolvedWorkspace: string;
    try { resolvedWorkspace = fs.realpathSync(workspace); } catch { resolvedWorkspace = path.resolve(workspace); }
    const wsPrefix = resolvedWorkspace.endsWith(path.sep) ? resolvedWorkspace : resolvedWorkspace + path.sep;
    if (real !== resolvedWorkspace && !real.startsWith(wsPrefix)) {
      return { error: `RFC path must be within the workspace (got ${raw})` };
    }

    if (!stat.isFile()) {
      return { error: `RFC path is not a file: ${raw}` };
    }

    return { path: real };
  } catch (err) {
    return { error: `could not resolve RFC path: ${(err as Error).message}` };
  }
}

// ─── RFC byte revision ────────────────────────────────────────────────────────

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

// ─── Step mutations (multi-map: stay in store) ────────────────────────────────

export function setPlan(cwd: string, steps: StepInput[], lifecycle: PlanPhase = 'executing'): PlanStep[] {
  const { normalizeInput: ni, dependencyIdsFromIndexes: difi } = { normalizeInput, dependencyIdsFromIndexes };
  const cleaned = steps.map(ni).filter((step) => step.text).slice(0, MAX_STEPS);
  const next: PlanStep[] = cleaned.map((step) => {
    const { dependsOn, ...stable } = step;
    const dependsOnStepIds = difi(dependsOn, cleaned, step.id);
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
  persist(cwd);
}

// ─── Bridge exports for plan-lifecycle.ts ────────────────────────────────────

/** Build a failed-transition result. For use by plan-lifecycle.ts only. */
export function planBuildTransitionError(cwd: string, code: PlanReviewTransitionCode, message: string): PlanReviewTransitionResult {
  return { ok: false, code, message, state: getPlanReviewState(cwd), steps: getPlan(cwd) };
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

// ─── Bridge exports for plan-executor.ts ─────────────────────────────────────

/** Set the raw steps Map. For use by plan-executor.ts only. */
export function planSetRawSteps(cwd: string, steps: PlanStep[]): void {
  plans.set(cwd, steps);
}

/** Set the raw lifecycle Map. For use by plan-executor.ts only. */
export function planSetRawLifecycle(cwd: string, phase: PlanPhase): void {
  planLifecycle.set(cwd, phase);
}

/** Delete from clearedScopes. For use by plan-executor.ts only. */
export function planDeleteClearedScope(cwd: string): void {
  clearedScopes.delete(cwd);
}

/** Call markUpdated. For use by plan-executor.ts only. */
export function planMarkUpdated(cwd: string): void {
  markUpdated(cwd);
}

/** Call persist. For use by plan-executor.ts only. */
export function planRunPersist(cwd: string): void {
  persist(cwd);
}

/** Call ensureLoaded. For use by plan-executor.ts only. */
export function planRunEnsureLoaded(cwd: string): void {
  ensureLoaded(cwd);
}

