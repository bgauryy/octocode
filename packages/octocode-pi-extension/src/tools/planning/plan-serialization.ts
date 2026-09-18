import { randomUUID } from 'node:crypto';
import type { PlanPhase } from '../plan-domain.js';
import {
  cleanContractText,
  cleanDecision,
  cleanPaths,
  cleanReviewText,
  cleanStepIds,
  MAX_PLAN_STEPS,
} from './plan-normalization.js';
import { workspaceForPlanScope, type PlanScope } from './plan-scope.js';
import type {
  PlanCoordination,
  PlanCoordinationMode,
  PlanDecision,
  PlanReviewComment,
  PlanStep,
  ReviewQuestion,
  ReviewState,
} from './plan-types.js';

const MAX_DECISIONS = 20;
const MAX_REVIEW_ITEMS = 100;

export interface PlanStored {
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

function readOptionalTimestamp(record: Record<string, unknown>, key: string): string | undefined {
  const value = record[key];
  return typeof value === 'string' && Number.isFinite(Date.parse(value)) ? value : undefined;
}

export function sanitizeStoredPlan(raw: unknown): PlanStep[] {
  if (!raw || typeof raw !== 'object' || !Array.isArray((raw as { steps?: unknown }).steps)) return [];
  const record = raw as Record<string, unknown>;
  if (record.version !== 4) return [];
  const out: PlanStep[] = [];
  for (const item of record.steps as unknown[]) {
    if (!item || typeof item !== 'object') continue;
    const source = item as Record<string, unknown>;
    const id = cleanContractText(source.id, 256);
    const text = cleanContractText(source.text);
    if (!id || !text) continue;
    const status = source.status === 'todo' || source.status === 'doing' || source.status === 'done' ? source.status : 'todo';
    const activeForm = cleanContractText(source.activeForm);
    const dependsOnStepIds = cleanStepIds(source.dependsOnStepIds);
    const paths = cleanPaths(source.paths);
    const reasoning = cleanContractText(source.reasoning);
    const acceptance = cleanContractText(source.acceptance);
    const checkCommand = cleanContractText(source.checkCommand);
    const awarenessTaskId = cleanContractText(source.awarenessTaskId, 256);
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
    if (out.length >= MAX_PLAN_STEPS) break;
  }
  return out;
}

function readQuestions(raw: unknown): ReviewQuestion[] {
  const value = raw && typeof raw === 'object' ? (raw as Record<string, unknown>).blockingQuestions : undefined;
  if (!Array.isArray(value)) return [];
  return value.flatMap((item): ReviewQuestion[] => {
    if (!item || typeof item !== 'object') return [];
    const source = item as Record<string, unknown>;
    const id = cleanReviewText(source.id);
    const prompt = cleanReviewText(source.prompt);
    if (!id || !prompt) return [];
    const answer = cleanReviewText(source.answer);
    return [{ id, prompt, blocking: source.blocking !== false, ...(answer ? { answer } : {}) }];
  }).slice(0, MAX_REVIEW_ITEMS);
}

function readComments(raw: unknown): PlanReviewComment[] {
  const value = raw && typeof raw === 'object' ? (raw as Record<string, unknown>).comments : undefined;
  if (!Array.isArray(value)) return [];
  return value.flatMap((item): PlanReviewComment[] => {
    if (!item || typeof item !== 'object') return [];
    const source = item as Record<string, unknown>;
    const id = cleanReviewText(source.id);
    const body = cleanReviewText(source.body);
    if (!id || !body) return [];
    const section = cleanReviewText(source.section);
    return [{ id, body, blocking: source.blocking !== false, resolved: source.resolved === true, ...(section ? { section } : {}) }];
  }).slice(0, MAX_REVIEW_ITEMS);
}

export function readCoordinationFromStored(raw: unknown, scope: PlanScope): PlanCoordination {
  const root = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {};
  const value = root.coordination && typeof root.coordination === 'object'
    ? root.coordination as Record<string, unknown>
    : {};
  const mode: PlanCoordinationMode = value.mode === 'required' || value.mode === 'local' ? value.mode : 'auto';
  const sourcePlanKey = cleanContractText(value.sourcePlanKey, 256) ?? `pi-plan-${randomUUID()}`;
  const coordinationWorkspace = cleanContractText(value.coordinationWorkspace, 2_000) ?? workspaceForPlanScope(scope);
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

export function readRfcFromStored(raw: unknown): string | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const value = (raw as Record<string, unknown>).rfcPath;
  return typeof value === 'string' && value.trim() ? value : undefined;
}

export function readDecisionsFromStored(raw: unknown): PlanDecision[] | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const value = (raw as Record<string, unknown>).decisions;
  if (!Array.isArray(value)) return undefined;
  const out: PlanDecision[] = [];
  for (const decision of value) {
    if (!decision || typeof decision !== 'object') continue;
    const source = decision as Record<string, unknown>;
    const q = typeof source.q === 'string' ? cleanDecision(source.q) : '';
    const a = typeof source.a === 'string' ? cleanDecision(source.a) : '';
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

export function readLifecycleFromStored(raw: unknown): PlanPhase {
  if (!raw || typeof raw !== 'object') return 'executing';
  const value = (raw as Record<string, unknown>).phase;
  return typeof value === 'string' && PLAN_PHASE_SET.has(value as PlanPhase) ? value as PlanPhase : 'executing';
}

export function reviewMetadataFromStored(raw: unknown): Omit<ReviewState, 'phase' | 'rfcPath' | 'decisions'> {
  if (!raw || typeof raw !== 'object') {
    return { branchSnapshotId: `plan-${randomUUID()}`, generation: 0, blockingQuestions: [], comments: [] };
  }
  const source = raw as Record<string, unknown>;
  const branchSnapshotId = typeof source.branchSnapshotId === 'string' && source.branchSnapshotId.trim()
    ? source.branchSnapshotId
    : `plan-${randomUUID()}`;
  const generation = Number.isSafeInteger(source.generation) && Number(source.generation) >= 0 ? Number(source.generation) : 0;
  const revision = cleanContractText(source.revision, 256);
  const acceptedRevision = cleanContractText(source.acceptedRevision, 256);
  const acceptAuthorizationReceiptId = typeof source.acceptAuthorizationReceiptId === 'string' && source.acceptAuthorizationReceiptId.trim() ? source.acceptAuthorizationReceiptId : undefined;
  const startAuthorizationReceiptId = typeof source.startAuthorizationReceiptId === 'string' && source.startAuthorizationReceiptId.trim() ? source.startAuthorizationReceiptId : undefined;
  const outcomeReason = cleanContractText(source.outcomeReason);
  const acceptedAt = readOptionalTimestamp(source, 'acceptedAt');
  const startedAt = readOptionalTimestamp(source, 'startedAt');
  return {
    branchSnapshotId,
    generation,
    ...(revision ? { revision } : {}),
    ...(acceptedRevision ? { acceptedRevision } : {}),
    ...(acceptAuthorizationReceiptId ? { acceptAuthorizationReceiptId } : {}),
    ...(startAuthorizationReceiptId ? { startAuthorizationReceiptId } : {}),
    ...(acceptedAt ? { acceptedAt } : {}),
    ...(startedAt ? { startedAt } : {}),
    ...(outcomeReason ? { outcomeReason } : {}),
    blockingQuestions: readQuestions(source),
    comments: readComments(source),
  };
}
