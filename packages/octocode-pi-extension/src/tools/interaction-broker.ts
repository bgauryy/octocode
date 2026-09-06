import { randomUUID } from 'node:crypto';
import { type AuthorizationReceiptV1, type InteractionAnswerV1, type InteractionRequestV1, type OutboxEventV1, type StoredInteractionV1 } from '@octocodeai/octocode-awareness';
import type { PiContext } from '../types.js';
import { isPersistentStorageEnabled } from '@octocodeai/config';
import { openPersistentAwareness } from './storage-policy.js';

interface InteractionStore {
  createInteraction(request: InteractionRequestV1): unknown;
  answerInteraction(answer: InteractionAnswerV1): unknown;
  deleteInteraction?(interactionId: string): unknown;
  getInteraction?(interactionId: string): StoredInteractionV1;
  createAuthorizationReceipt?(receipt: AuthorizationReceiptV1): unknown;
  consumeAuthorizationReceipt?(params: { receiptId: string; planId: string; revision: string; scope: string }): unknown;
  listPendingInteractions?(params?: { sessionId?: string; limit?: number }): Array<{ request: InteractionRequestV1 }>;
  listEvents?(params: { consumerId: string; limit?: number }): OutboxEventV1[];
  acknowledgeEvent?(params: { consumerId: string; eventId: string; decision: 'accept' | 'hold' | 'refuse' }): unknown;
  close(): void;
}

type InteractionStoreFactory = (workspace: string) => InteractionStore;
function createEphemeralInteractionStore(): InteractionStore {
  return {
    createInteraction: () => undefined,
    answerInteraction: () => undefined,
    createAuthorizationReceipt: () => undefined,
    consumeAuthorizationReceipt: () => undefined,
    listPendingInteractions: () => [],
    listEvents: () => [],
    acknowledgeEvent: () => undefined,
    close: () => undefined,
  };
}

interface EphemeralWorkspaceState {
  interactions: Map<string, StoredInteractionV1>;
  receipts: Map<string, AuthorizationReceiptV1>;
}

const ephemeralWorkspaces = new Map<string, EphemeralWorkspaceState>();

export interface InMemoryInteractionCleanupResult {
  workspaces: number;
  interactions: number;
  receipts: number;
}

function isExpired(expiresAt: string | undefined, now: number): boolean {
  return expiresAt !== undefined && Date.parse(expiresAt) <= now;
}

function pruneExpiredEntries(state: EphemeralWorkspaceState, now = Date.now()): void {
  for (const [interactionId, stored] of state.interactions) {
    if (isExpired(stored.request.expiresAt, now)) state.interactions.delete(interactionId);
  }
  for (const [receiptId, receipt] of state.receipts) {
    if (isExpired(receipt.expiresAt, now)) state.receipts.delete(receiptId);
  }
}

function releaseEmptyWorkspace(workspace: string, state: EphemeralWorkspaceState): boolean {
  if (state.interactions.size > 0 || state.receipts.size > 0) return false;
  return ephemeralWorkspaces.get(workspace) === state && ephemeralWorkspaces.delete(workspace);
}

/** Release memory-mode state globally, per workspace, or for one session. */
export function clearInMemoryInteractionState(
  options: { workspace?: string; sessionId?: string } = {},
): InMemoryInteractionCleanupResult {
  const result: InMemoryInteractionCleanupResult = { workspaces: 0, interactions: 0, receipts: 0 };
  const workspaces = options.workspace
    ? [[options.workspace, ephemeralWorkspaces.get(options.workspace)] as const]
    : [...ephemeralWorkspaces.entries()].map(([workspace, state]) => [workspace, state] as const);

  for (const [workspace, state] of workspaces) {
    if (!state) continue;
    for (const [interactionId, stored] of state.interactions) {
      if (options.sessionId && stored.request.sessionId !== options.sessionId) continue;
      state.interactions.delete(interactionId);
      result.interactions += 1;
    }
    for (const [receiptId, receipt] of state.receipts) {
      if (options.sessionId && receipt.sessionId !== options.sessionId) continue;
      state.receipts.delete(receiptId);
      result.receipts += 1;
    }
    if (releaseEmptyWorkspace(workspace, state)) result.workspaces += 1;
  }
  return result;
}

function createInMemoryInteractionStore(workspace: string): InteractionStore {
  let state = ephemeralWorkspaces.get(workspace);
  if (!state) {
    state = { interactions: new Map(), receipts: new Map() };
    ephemeralWorkspaces.set(workspace, state);
  }
  const workspaceState = state;
  return {
    createInteraction: (request) => {
      workspaceState.interactions.set(
        request.interactionId,
        { request } as StoredInteractionV1,
      );
    },
    answerInteraction: (answer) => {
      const current = workspaceState.interactions.get(answer.interactionId);
      if (!current) throw new Error('interaction request was not found');
      workspaceState.interactions.set(
        answer.interactionId,
        { ...current, answer } as StoredInteractionV1,
      );
    },
    deleteInteraction: (interactionId) => {
      workspaceState.interactions.delete(interactionId);
      releaseEmptyWorkspace(workspace, workspaceState);
    },
    getInteraction: (interactionId) => {
      const current = workspaceState.interactions.get(interactionId);
      if (!current) throw new Error('interaction request was not found');
      return current;
    },
    createAuthorizationReceipt: (receipt) => {
      workspaceState.receipts.set(receipt.receiptId, receipt);
    },
    consumeAuthorizationReceipt: ({ receiptId, planId, revision, scope }) => {
      const receipt = workspaceState.receipts.get(receiptId);
      if (
        !receipt
        || receipt.planId !== planId
        || receipt.revision !== revision
        || !receipt.scope.includes(scope)
        || (receipt.expiresAt !== undefined && Date.parse(receipt.expiresAt) <= Date.now())
      ) {
        throw new Error('authorization receipt is invalid or expired');
      }
      workspaceState.receipts.delete(receiptId);
    },
    listPendingInteractions: ({ sessionId, limit = 500 } = {}) => {
      pruneExpiredEntries(workspaceState);
      return [...workspaceState.interactions.values()]
        .filter((item) => !item.answer && (!sessionId || item.request.sessionId === sessionId))
        .slice(0, limit)
        .map((item) => ({ request: item.request }));
    },
    listEvents: () => [],
    acknowledgeEvent: () => undefined,
    close: () => {
      pruneExpiredEntries(workspaceState);
      releaseEmptyWorkspace(workspace, workspaceState);
    },
  };
}

const defaultStoreFactory: InteractionStoreFactory = (workspace) =>
  process.env['VITEST']
    ? createEphemeralInteractionStore()
    : isPersistentStorageEnabled()
      ? openPersistentAwareness({ workspace })
      : createInMemoryInteractionStore(workspace);
let storeFactory: InteractionStoreFactory = defaultStoreFactory;
const durableAnswerRoutes = new WeakMap<object, boolean>();

export function setInteractionStoreFactoryForTests(factory?: InteractionStoreFactory): void {
  storeFactory = factory ?? defaultStoreFactory;
}

/** Set by the composed host after it exposes a trusted answer submission route. */
export function configureInteractionBrokerRoute(ctx: PiContext, available: boolean): void {
  durableAnswerRoutes.set(ctx, available);
  if (typeof ctx.sessionManager === 'object' && ctx.sessionManager !== null) {
    durableAnswerRoutes.set(ctx.sessionManager, available);
  }
}

function hasDurableAnswerRoute(ctx: PiContext): boolean {
  return durableAnswerRoutes.get(ctx) === true
    || (typeof ctx.sessionManager === 'object'
      && ctx.sessionManager !== null
      && durableAnswerRoutes.get(ctx.sessionManager) === true);
}

export function brokerSessionId(ctx: PiContext): string {
  return ctx.sessionManager?.getSessionId?.()
    ?? ctx.sessionManager?.getSessionFile?.()
    ?? `host:${ctx.mode ?? 'unknown'}:${process.pid}`;
}

export function createHumanAuthorizationReceipt(
  ctx: PiContext | undefined,
  params: { planId: string; revision: string; scope: string; question: string; expiresInMs?: number },
): AuthorizationReceiptV1 {
  const hostContext = ctx ?? ({ cwd: process.cwd(), mode: 'rpc' } as PiContext);
  const request = createPendingInteraction(hostContext, {
    kind: 'authorization',
    question: params.question,
    options: [{ id: 'authorize', label: 'Authorize this exact revision' }],
    expiresInMs: params.expiresInMs ?? 15 * 60_000,
  });
  answerPendingInteraction(request, { status: 'selected', value: 'authorize' });
  const receipt: AuthorizationReceiptV1 = {
    version: 1,
    receiptId: `authorization_${randomUUID()}`,
    interactionId: request.interactionId,
    workspace: request.workspace,
    sessionId: request.sessionId,
    planId: params.planId,
    revision: params.revision,
    scope: [params.scope],
    actor: { kind: 'user', id: 'session-operator' },
    provenance: { source: 'session-operator', trust: 'authority' },
    createdAt: new Date().toISOString(),
    expiresAt: request.expiresAt,
  };
  const store = storeFactory(request.workspace);
  try {
    store.createAuthorizationReceipt?.(receipt);
    store.deleteInteraction?.(request.interactionId);
  } finally {
    store.close();
  }
  return receipt;
}

export function createHumanAuthorizationReceiptFromInteraction(
  ctx: PiContext,
  params: {
    interactionId: string;
    planId: string;
    revision: string;
    scope: string;
    expectedOptionId: string;
    /** Keep the answered interaction for a second tightly-bound receipt scope. */
    consumeInteraction?: boolean;
  },
): AuthorizationReceiptV1 {
  const workspace = ctx.cwd ?? process.cwd();
  const sessionId = brokerSessionId(ctx);
  const store = storeFactory(workspace);
  try {
    if (!store.getInteraction) throw new Error('interaction store cannot load authorization requests');
    if (!store.createAuthorizationReceipt) throw new Error('interaction store cannot create authorization receipts');
    const stored = store.getInteraction(params.interactionId);
    const request = stored.request;
    const answer = stored.answer;
    if (request.workspace !== workspace || request.sessionId !== sessionId) {
      throw new Error('authorization interaction does not belong to this workspace and session');
    }
    if (request.kind !== 'authorization') throw new Error('interaction is not an authorization request');
    if (request.expiresAt !== undefined && Date.parse(request.expiresAt) <= Date.now()) {
      throw new Error('authorization interaction is expired');
    }
    if (!answer || answer.cancelled || !answer.optionIds?.includes(params.expectedOptionId)) {
      throw new Error('authorization interaction does not contain the required human Start decision');
    }
    const receipt: AuthorizationReceiptV1 = {
      version: 1,
      receiptId: `authorization_${randomUUID()}`,
      interactionId: request.interactionId,
      workspace,
      sessionId,
      planId: params.planId,
      revision: params.revision,
      scope: [params.scope],
      actor: answer.actor,
      provenance: answer.provenance,
      createdAt: answer.createdAt,
      expiresAt: request.expiresAt,
    };
    store.createAuthorizationReceipt(receipt);
    if (params.consumeInteraction !== false) store.deleteInteraction?.(request.interactionId);
    return receipt;
  } finally {
    store.close();
  }
}

export function consumeHumanAuthorizationReceipt(
  workspace: string,
  params: { receiptId: string; planId: string; revision: string; scope: string },
): void {
  const store = storeFactory(workspace);
  try { store.consumeAuthorizationReceipt?.(params); } finally { store.close(); }
}

export function listPendingInteractionIds(ctx: PiContext): string[] {
  return listPendingInteractions(ctx).map((request) => request.interactionId);
}

export function listPendingInteractions(ctx: PiContext): InteractionRequestV1[] {
  const workspace = ctx.cwd ?? process.cwd();
  const store = storeFactory(workspace);
  try { return (store.listPendingInteractions?.({ sessionId: brokerSessionId(ctx), limit: 500 }) ?? []).map((item) => item.request); }
  finally { store.close(); }
}

export interface BrokerQuestionOption {
  id: string;
  label: string;
  description?: string;
  recommended?: boolean;
  disabledReason?: string;
}

export function createPendingInteraction(
  ctx: PiContext,
  params: { question: string; options: BrokerQuestionOption[]; kind?: 'question' | 'authorization'; expiresInMs?: number },
): InteractionRequestV1 {
  const workspace = ctx.cwd ?? process.cwd();
  const interactionId = `interaction_${randomUUID()}`;
  const createdAt = new Date().toISOString();
  const request: InteractionRequestV1 = {
    version: 1,
    interactionId,
    workspace,
    sessionId: brokerSessionId(ctx),
    correlationId: `correlation_${randomUUID()}`,
    kind: params.kind ?? 'question',
    question: params.question,
    options: params.options.length > 0 ? params.options : [{ id: 'free-text', label: 'Type an answer' }],
    status: 'pending',
    createdAt,
    expiresAt: new Date(Date.now() + (params.expiresInMs ?? 24 * 60 * 60_000)).toISOString(),
  };
  const store = storeFactory(workspace);
  try { store.createInteraction(request); } finally { store.close(); }
  return request;
}

export function answerPendingInteraction(
  request: InteractionRequestV1,
  outcome: { status: string; value?: string; values?: string[] | Record<string, string> },
): InteractionAnswerV1 {
  const selected = outcome.status === 'selected'
    ? [outcome.value!]
    : outcome.status === 'multiSelected' && Array.isArray(outcome.values) ? outcome.values : undefined;
  const typed = outcome.status === 'text'
    ? outcome.value
    : outcome.status === 'form' && outcome.values && !Array.isArray(outcome.values) ? JSON.stringify(outcome.values) : undefined;
  const answer: InteractionAnswerV1 = {
    version: 1,
    interactionId: request.interactionId,
    correlationId: request.correlationId,
    sessionId: request.sessionId,
    actor: { kind: 'user', id: 'session-operator' },
    provenance: { source: 'session-operator', trust: 'authority' },
    ...(selected?.length ? { optionIds: selected } : {}),
    ...(typed ? { text: typed } : {}),
    ...((outcome.status === 'back' || outcome.status === 'cancelled' || outcome.status === 'timed_out' || outcome.status === 'unavailable') ? { cancelled: true } : {}),
    createdAt: new Date().toISOString(),
  };
  const store = storeFactory(request.workspace);
  try {
    store.answerInteraction(answer);
    if (request.kind !== 'authorization' || answer.cancelled) {
      store.deleteInteraction?.(request.interactionId);
    }
  } finally {
    store.close();
  }
  return answer;
}

export interface HostInteractionAnswerV1 {
  version: 1;
  interactionId: string;
  correlationId: string;
  sessionId: string;
  outcome: { status: string; value?: string; values?: string[] | Record<string, string> };
}

/** Submit an external/RPC answer against the canonical stored request. */
export function submitHostInteractionAnswer(ctx: PiContext, input: HostInteractionAnswerV1): InteractionAnswerV1 {
  const workspace = ctx.cwd ?? process.cwd();
  const sessionId = brokerSessionId(ctx);
  if (input.version !== 1) throw new Error('interaction answer version is unsupported');
  if (input.sessionId !== sessionId) throw new Error('interaction answer session mismatch');
  validateHostOutcome(input.outcome);
  const store = storeFactory(workspace);
  let stored: StoredInteractionV1;
  try {
    if (!store.getInteraction) throw new Error('interaction store cannot load requests');
    stored = store.getInteraction(input.interactionId);
  } finally {
    store.close();
  }
  if (stored.request.workspace !== workspace) throw new Error('interaction answer workspace mismatch');
  if (stored.request.sessionId !== sessionId) throw new Error('interaction answer session mismatch');
  if (stored.request.correlationId !== input.correlationId) throw new Error('interaction answer correlation mismatch');
  return answerPendingInteraction(stored.request, input.outcome);
}

function validateHostOutcome(outcome: HostInteractionAnswerV1['outcome']): void {
  switch (outcome.status) {
    case 'selected':
      if (!outcome.value?.trim()) throw new Error('selected interaction answer requires a value');
      return;
    case 'text':
      if (!outcome.value?.trim()) throw new Error('text interaction answer requires a value');
      return;
    case 'multiSelected':
      if (!Array.isArray(outcome.values) || outcome.values.length === 0 || outcome.values.some((value) => !value.trim())) {
        throw new Error('multiSelected interaction answer requires values');
      }
      return;
    case 'form':
      if (!outcome.values || Array.isArray(outcome.values)) throw new Error('form interaction answer requires fields');
      return;
    case 'back':
    case 'cancelled':
      return;
    default:
      throw new Error(`interaction answer status is unsupported: ${outcome.status}`);
  }
}

export interface InteractionContinuationV1 {
  version: 1;
  continuationId: string;
  interactionId: string;
  correlationId: string;
  sessionId: string;
  status: 'answered' | 'cancelled';
  answer: InteractionAnswerV1;
}

export interface InteractionContinuationDrainResult {
  consumerId: string;
  delivered: number;
  skipped: number;
  lastSequence: number;
}

/**
 * Deliver answered interactions in durable outbox order. Delivery happens
 * before acknowledgement: a crash causes safe re-delivery with the same
 * continuationId; a successful ack suppresses future duplicates after restart.
 */
export async function drainInteractionContinuations(
  ctx: PiContext,
  deliver: (continuation: InteractionContinuationV1) => void | Promise<void>,
  options: { consumerId?: string; limit?: number } = {},
): Promise<InteractionContinuationDrainResult> {
  const workspace = ctx.cwd ?? process.cwd();
  const sessionId = brokerSessionId(ctx);
  const consumerId = options.consumerId?.trim() || `interaction-host:${sessionId}`;
  const store = storeFactory(workspace);
  let delivered = 0;
  let skipped = 0;
  let lastSequence = 0;
  try {
    if (!store.listEvents || !store.acknowledgeEvent) throw new Error('interaction store cannot consume continuation events');
    const events = store.listEvents({ consumerId, limit: options.limit ?? 100 });
    for (const event of events) {
      lastSequence = event.sequence;
      const continuation = continuationFromEvent(event, sessionId);
      if (!continuation) {
        store.acknowledgeEvent({ consumerId, eventId: event.eventId, decision: 'refuse' });
        skipped += 1;
        continue;
      }
      await deliver(continuation);
      store.acknowledgeEvent({ consumerId, eventId: event.eventId, decision: 'accept' });
      delivered += 1;
    }
  } finally {
    store.close();
  }
  return { consumerId, delivered, skipped, lastSequence };
}

function continuationFromEvent(event: OutboxEventV1, sessionId: string): InteractionContinuationV1 | undefined {
  if (event.sessionId !== sessionId || (event.type !== 'question.answered' && event.type !== 'question.cancelled')) return undefined;
  const answer = event.payload as InteractionAnswerV1;
  if (!event.aggregate || !answer || answer.version !== 1 || answer.sessionId !== sessionId || answer.interactionId !== event.aggregate.id) return undefined;
  return {
    version: 1,
    continuationId: event.eventId,
    interactionId: answer.interactionId,
    correlationId: answer.correlationId,
    sessionId,
    status: answer.cancelled ? 'cancelled' : 'answered',
    answer,
  };
}

export function shouldBrokerInteraction(ctx: PiContext | undefined): ctx is PiContext {
  return Boolean(
    ctx
    && hasDurableAnswerRoute(ctx)
    && (ctx.mode !== 'tui' || ctx.sessionManager?.getSessionId?.() || ctx.sessionManager?.getSessionFile?.()),
  );
}
