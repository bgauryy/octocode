import type { PlanPhase } from './plan-domain.js';
import path from 'node:path';
import { contentDigest, type ContextSegmentV1 } from '@octocodeai/octocode-awareness';
import type { PiContext, PiInstance } from '../types.js';
import {
  activePlanScope,
} from './active-plan.js';
import { getCurrentPlanReadModel, renderPlanContext } from './plan-read-model.js';
import { setManagedActivity } from './runtime-renderer.js';
import { brokerSessionId } from './interaction-broker.js';
import { canReprojectRehydratedSegment } from './prompt-lifecycle.js';
import { resolveSessionCheckpointSources } from './context-source-registry.js';
import {
  createSessionArtifactContext,
  inspectRehydrationLedger,
  resolveRehydrationContentRefs,
  type RehydrationLedgerV1,
} from './session-artifacts.js';
import { isPersistentStorageEnabled } from '@octocodeai/config';
import { openPersistentAwareness } from './storage-policy.js';

export const REHYDRATION_RECEIPT_ENTRY_TYPE = 'octocode-rehydration-receipt';

export type RehydrationReason = 'new' | 'resume' | 'fork' | 'tree' | 'compaction' | string;
export const REHYDRATION_PROJECTION_TOKEN_BUDGET = 8_000;
export type RehydrationOutcome = 'pending-validation' | 'restored' | 'expired' | 'missing' | 'corrupt' | 'identity-mismatch' | 'fork-reset';

export interface LivePlanRecoveryState {
  scope: string;
  branchSnapshotId: string;
  generation: number;
  revision?: string;
  acceptedRevision?: string;
  phase: PlanPhase;
  content: string;
  pendingInteractionIds?: string[];
}

interface ContinuityReader {
  listPendingInteractions(params?: { sessionId?: string; limit?: number }): Array<{ request: { interactionId: string } }>;
  getConsumerCursor(consumerId: string): number;
  close(): void;
}

export interface RehydrationDependencies {
  getLivePlan?(ctx: PiContext): LivePlanRecoveryState | undefined;
  openContinuity?(workspace: string): ContinuityReader;
  setActivity?(activity: Parameters<typeof setManagedActivity>[1]): void;
  now?(): number;
  totalTokenBudget?: number;
}

export interface RehydrationReceiptV1 {
  version: 1;
  reason: RehydrationReason;
  outcome: RehydrationOutcome;
  sessionKey: string;
  capturedAt?: string;
  present: string[];
  validated: string[];
  reprojected: string[];
  restored: string[];
  stale: string[];
  skipped: string[];
  corrupt: string[];
  overBudget: string[];
  estimatedTokens: number;
  planDecision: 'match' | 'live-newer' | 'ledger-only' | 'none';
  pendingInteractionIds: string[];
  consumerCursors: Record<string, { live: number; hint: number; decision: 'match' | 'live-ahead' | 'held' }>;
  recordedAt: string;
}

export interface CurrentRehydrationSource {
  segment: ContextSegmentV1;
  content: string;
}

export interface ValidatedRehydrationProjection {
  content: string;
  segments: ContextSegmentV1[];
  receipt: RehydrationReceiptV1;
}

interface PendingRehydration {
  ledger: RehydrationLedgerV1;
  present: string[];
  corrupt: string[];
  reason: RehydrationReason;
  planMatches: boolean;
  baseReceipt: RehydrationReceiptV1;
}

const pendingBySession = new Map<string, PendingRehydration>();

/**
 * Drop process-local staged recovery state at a session lifecycle boundary.
 * Pi may provide a stale replacement-session context during shutdown, so this
 * intentionally accepts no context and never derives an identity from one.
 * The extension owns one live session; rehydrateSession enforces the same
 * single-entry bound when a new identity is staged.
 */
export function clearPendingRehydration(): boolean {
  const hadPending = pendingBySession.size > 0;
  pendingBySession.clear();
  return hadPending;
}

function defaultLivePlan(ctx: PiContext): LivePlanRecoveryState | undefined {
  const scope = activePlanScope(ctx);
  const model = getCurrentPlanReadModel(ctx, scope);
  const content = renderPlanContext(model);
  if (!content && model.phase === 'abandoned') return undefined;
  return {
    scope,
    branchSnapshotId: model.review.branchSnapshotId,
    generation: model.review.generation,
    ...(model.revision ? { revision: model.revision } : {}),
    ...(model.acceptedRevision ? { acceptedRevision: model.acceptedRevision } : {}),
    phase: model.phase,
    content,
    pendingInteractionIds: model.pendingInteractionIds,
  };
}

function activityFor(plan: LivePlanRecoveryState | undefined): Parameters<typeof setManagedActivity>[1] {
  if (!plan) return { kind: 'idle' };
  if (plan.pendingInteractionIds?.length) {
    return {
      kind: 'awaiting_input',
      planScope: plan.scope,
      question: `Pending interaction${plan.pendingInteractionIds.length === 1 ? '' : 's'}: ${plan.pendingInteractionIds.join(', ')}`,
    };
  }
  switch (plan.phase) {
    case 'researching': return { kind: 'researching', planScope: plan.scope, detail: 'Restoring plan research' };
    case 'needs_answers': return { kind: 'awaiting_input', planScope: plan.scope, question: 'Plan clarification required' };
    case 'draft':
    case 'in_review': return { kind: 'reviewing', planScope: plan.scope, ...(plan.revision ? { revision: plan.revision } : {}) };
    case 'accepted': return { kind: 'awaiting_start', planScope: plan.scope, revision: plan.acceptedRevision ?? plan.revision ?? 'unversioned' };
    case 'executing': return { kind: 'working', planScope: plan.scope, label: 'Resuming authorized plan' };
    case 'verifying': return { kind: 'verifying', planScope: plan.scope, label: 'Restoring verification state' };
    case 'blocked': return { kind: 'blocked', label: 'Plan blocked' };
    case 'complete': return { kind: 'complete', label: 'Plan complete' };
    case 'failed': return { kind: 'failed', label: 'Plan failed' };
    case 'abandoned': return { kind: 'idle' };
  }
}

function samePlan(
  ledgerPlan: { scope: string; branchSnapshotId: string; generation: number; revision?: string } | undefined,
  livePlan: LivePlanRecoveryState | undefined,
): boolean {
  if (!ledgerPlan || !livePlan) return false;
  return ledgerPlan.scope === livePlan.scope
    && ledgerPlan.branchSnapshotId === livePlan.branchSnapshotId
    && ledgerPlan.generation === livePlan.generation
    && (ledgerPlan.revision ?? '') === (livePlan.revision ?? '');
}

/**
 * Reconcile a checkpoint against live durable authorities. This function never
 * writes plan, interaction, event-cursor, or authorization rows: a ledger is a
 * recovery hint and therefore cannot replace newer branch/SQLite state.
 */
export function rehydrateSession(
  ctx: PiContext,
  reason: RehydrationReason,
  dependencies: RehydrationDependencies = {},
): RehydrationReceiptV1 {
  const artifact = createSessionArtifactContext(ctx);
  // Only one Pi session is live in this extension process. Replacing the staged
  // slot here prevents abandoned or malformed identities from accumulating and
  // ensures a newly staged session supersedes any older unconsumed checkpoint.
  clearPendingRehydration();
  const now = dependencies.now ?? Date.now;
  const livePlan = (dependencies.getLivePlan ?? defaultLivePlan)(ctx);
  (dependencies.setActivity ?? ((activity) => setManagedActivity(ctx, activity)))(activityFor(livePlan));
  const base = {
    version: 1 as const,
    reason,
    sessionKey: artifact.identity.sessionKey,
    present: [] as string[], validated: [] as string[], reprojected: [] as string[],
    restored: [] as string[], stale: [] as string[], skipped: [] as string[], corrupt: [] as string[], overBudget: [] as string[],
    estimatedTokens: 0,
    planDecision: 'none' as RehydrationReceiptV1['planDecision'],
    pendingInteractionIds: [] as string[],
    consumerCursors: {} as RehydrationReceiptV1['consumerCursors'],
    recordedAt: new Date(now()).toISOString(),
  };
  // A fork owns a new identity and may retain a reviewable branch snapshot, but
  // it must never search for or consume the parent's checkpoint or authority.
  if (reason === 'fork') {
    return { ...base, outcome: 'fork-reset' };
  }

  const inspected = inspectRehydrationLedger(artifact);
  if (inspected.status !== 'valid') return { ...base, outcome: inspected.status };
  const ledger = inspected.ledger;
  const refs = resolveRehydrationContentRefs(artifact, ledger);
  const planMatches = samePlan(ledger.plan, livePlan);
  let planDecision: RehydrationReceiptV1['planDecision'] = 'none';
  if (ledger.plan && livePlan) planDecision = planMatches ? 'match' : 'live-newer';
  else if (ledger.plan) planDecision = 'ledger-only';

  const workspace = path.resolve(ctx.cwd ?? process.cwd());
  const sessionId = brokerSessionId(ctx);
  const openContinuity = dependencies.openContinuity ?? (
    isPersistentStorageEnabled()
      ? (value: string) => openPersistentAwareness({ workspace: value })
      : () => ({
          listPendingInteractions: () => [],
          getConsumerCursor: () => 0,
          close: () => undefined,
        })
  );
  const continuity = openContinuity(workspace);
  let pendingInteractionIds: string[] = [];
  const consumerCursors: RehydrationReceiptV1['consumerCursors'] = {};
  try {
    pendingInteractionIds = continuity.listPendingInteractions({ ...(sessionId ? { sessionId } : {}), limit: 500 })
      .map((item) => item.request.interactionId)
      .filter(Boolean)
      .sort();
    for (const [consumerId, hint] of Object.entries(ledger.consumerCursors).sort(([a], [b]) => a.localeCompare(b))) {
      const live = continuity.getConsumerCursor(consumerId);
      consumerCursors[consumerId] = {
        live,
        hint,
        decision: live === hint ? 'match' : live > hint ? 'live-ahead' : 'held',
      };
    }
  } finally {
    continuity.close();
  }

  const receipt: RehydrationReceiptV1 = {
    ...base,
    outcome: Date.parse(ledger.expiresAt) <= now() ? 'expired' : 'pending-validation',
    capturedAt: ledger.capturedAt,
    present: Object.keys(refs.contents).sort(),
    stale: !planMatches && ledger.segments.some((segment) => segment.id === 'active-plan') ? ['active-plan'] : [],
    skipped: [],
    corrupt: refs.corrupt,
    planDecision,
    pendingInteractionIds,
    consumerCursors,
  };
  if (receipt.outcome === 'pending-validation') {
    pendingBySession.set(artifact.identity.sessionKey, {
      ledger,
      present: receipt.present,
      corrupt: receipt.corrupt,
      reason,
      planMatches,
      baseReceipt: receipt,
    });
  }
  return receipt;
}

function sameSegmentIdentity(checkpoint: ContextSegmentV1, current: ContextSegmentV1): boolean {
  return checkpoint.id === current.id
    && checkpoint.kind === current.kind
    && checkpoint.origin === current.origin
    && checkpoint.authority === current.authority
    && checkpoint.scope === current.scope
    && checkpoint.visibility === current.visibility
    && checkpoint.rehydrate === current.rehydrate;
}

/** Validate a staged checkpoint against its current content owners and consume
 * it atomically. Saved artifact bodies are never returned or projected. */
export function consumeValidatedRehydration(
  ctx: PiContext,
  currentSources: CurrentRehydrationSource[],
  options: { allowProjection?: boolean; totalTokenBudget?: number; now?: () => number } = {},
): ValidatedRehydrationProjection | undefined {
  const sessionKey = createSessionArtifactContext(ctx).identity.sessionKey;
  const pending = pendingBySession.get(sessionKey);
  if (!pending) return undefined;
  pendingBySession.delete(sessionKey);
  const now = options.now ?? Date.now;
  const sessionSources = resolveSessionCheckpointSources(ctx, pending.ledger.segments);
  const currentById = new Map([...sessionSources, ...currentSources].map((source) => [source.segment.id, source]));
  const validated: string[] = [];
  const reprojected: string[] = [];
  const stale = [...pending.baseReceipt.stale];
  const skipped: string[] = [];
  const overBudget: string[] = [];
  const segments: ContextSegmentV1[] = [];
  const blocks: string[] = [];
  let estimatedTokens = 0;
  const expired = Date.parse(pending.ledger.expiresAt) <= now();
  for (const checkpoint of pending.ledger.segments) {
    if (pending.corrupt.includes(checkpoint.id) || !pending.present.includes(checkpoint.id)) {
      skipped.push(checkpoint.id);
      continue;
    }
    const current = currentById.get(checkpoint.id);
    if (expired || !current || !sameSegmentIdentity(checkpoint, current.segment)
      || checkpoint.digest !== contentDigest(current.content)
      || current.segment.digest !== checkpoint.digest
      || (checkpoint.id === 'active-plan' && !pending.planMatches)) {
      if (!stale.includes(checkpoint.id)) stale.push(checkpoint.id);
      continue;
    }
    const tokens = Math.ceil(current.content.length / 4);
    if ((checkpoint.tokenBudget !== undefined && tokens > checkpoint.tokenBudget)
      || estimatedTokens + tokens > (options.totalTokenBudget ?? REHYDRATION_PROJECTION_TOKEN_BUDGET)) {
      overBudget.push(checkpoint.id);
      continue;
    }
    validated.push(checkpoint.id);
    const mayProject = options.allowProjection === true
      && canReprojectRehydratedSegment(checkpoint);
    if (!mayProject) {
      skipped.push(checkpoint.id);
      continue;
    }
    const label = `<rehydrated_segment id=${JSON.stringify(checkpoint.id)} origin=${JSON.stringify(checkpoint.origin)} authority=${JSON.stringify(checkpoint.authority)} source="current">`;
    const block = `${label}\n${current.content}\n</rehydrated_segment>`;
    const projectedTokens = Math.ceil(block.length / 4);
    if (estimatedTokens + projectedTokens > (options.totalTokenBudget ?? REHYDRATION_PROJECTION_TOKEN_BUDGET)) {
      overBudget.push(checkpoint.id);
      continue;
    }
    estimatedTokens += projectedTokens;
    reprojected.push(checkpoint.id);
    segments.push(current.segment);
    blocks.push(block);
  }
  const receipt: RehydrationReceiptV1 = {
    ...pending.baseReceipt,
    outcome: expired ? 'expired' : 'restored',
    validated: validated.sort(),
    reprojected: reprojected.sort(),
    restored: reprojected.sort(),
    stale: [...new Set(stale)].sort(),
    skipped: [...new Set(skipped)].sort(),
    overBudget: [...new Set(overBudget)].sort(),
    estimatedTokens,
    recordedAt: new Date(now()).toISOString(),
  };
  return { content: blocks.join('\n\n'), segments, receipt };
}

/** Persist a body-safe receipt in Pi's state channel; segment contents never
 * enter the receipt or transcript. */
export function runAndRecordRehydration(
  pi: PiInstance,
  ctx: PiContext,
  reason: RehydrationReason,
  dependencies: RehydrationDependencies = {},
): RehydrationReceiptV1 {
  const receipt = rehydrateSession(ctx, reason, dependencies);
  pi.appendEntry?.(REHYDRATION_RECEIPT_ENTRY_TYPE, receipt);
  return receipt;
}
