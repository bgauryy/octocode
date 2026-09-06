import { createHash } from 'node:crypto';

export const ACTOR_KINDS = ['user', 'agent', 'hook', 'system', 'tool', 'memory', 'mcp'] as const;
export const PROVENANCE_SOURCES = ['harness', 'session-operator', 'peer', 'hook', 'tool', 'memory', 'mcp'] as const;
export const TRUST_CLASSES = ['authority', 'attributed-data'] as const;
export const INBOUND_DECISIONS = ['accept', 'hold', 'refuse'] as const;

export type ActorKind = typeof ACTOR_KINDS[number];
export type ProvenanceSource = typeof PROVENANCE_SOURCES[number];
export type TrustClass = typeof TRUST_CLASSES[number];
export type InboundDecision = typeof INBOUND_DECISIONS[number];
export type PeerMessageClass = 'informational' | 'blocking' | 'proposal' | 'handoff';

export interface PeerInboundPolicyResultV1 {
  version: 1;
  decision: InboundDecision;
  messageClass: PeerMessageClass;
  reason: string;
  attributedText?: string;
}

export interface ActorIdentityV1 { kind: ActorKind; id: string }
export interface EventProvenanceV1 { source: ProvenanceSource; trust: TrustClass }

export interface AgentEventEnvelopeV1<T = unknown> {
  version: 1;
  eventId: string;
  workspace: string;
  sessionId?: string;
  correlationId?: string;
  type: string;
  actor: ActorIdentityV1;
  provenance: EventProvenanceV1;
  aggregate?: { kind: string; id: string; revision?: string };
  createdAt: string;
  expiresAt?: string;
  payload: T;
}

export interface AuthorizationReceiptV1 {
  version: 1;
  receiptId: string;
  interactionId: string;
  workspace: string;
  sessionId: string;
  planId: string;
  revision: string;
  scope: string[];
  actor: ActorIdentityV1;
  provenance: EventProvenanceV1;
  createdAt: string;
  expiresAt?: string;
  consumedAt?: string;
}

export interface InteractionRequestV1 {
  version: 1;
  interactionId: string;
  workspace: string;
  sessionId: string;
  correlationId: string;
  kind: 'question' | 'authorization';
  question: string;
  options: Array<{ id: string; label: string; description?: string; recommended?: boolean; disabledReason?: string }>;
  status: 'pending' | 'answered' | 'cancelled' | 'expired';
  createdAt: string;
  expiresAt?: string;
}

export interface InteractionAnswerV1 {
  version: 1;
  interactionId: string;
  correlationId: string;
  sessionId: string;
  actor: ActorIdentityV1;
  provenance: EventProvenanceV1;
  optionIds?: string[];
  text?: string;
  cancelled?: boolean;
  createdAt: string;
}

export type ContextAuthority = 'product' | 'user' | 'project' | 'external-data';
export interface ContextSegmentV1 {
  version: 1;
  id: string;
  kind: 'product-policy' | 'user-request' | 'project-instruction' | 'skill' | 'plan' | 'memory-lead' | 'tool-contract' | 'tool-result' | 'peer-event';
  origin: string;
  authority: ContextAuthority;
  digest: string;
  scope: 'session' | 'turn' | 'task' | 'path';
  visibility: 'hidden-policy' | 'inspectable' | 'transcript';
  rehydrate: 'always' | 'on-trigger' | 'summary-only' | 'never';
  tokenBudget?: number;
}

export interface CapabilityDecisionReceiptV1 {
  version: 1;
  receiptId: string;
  action: string;
  resource: string;
  actor: ActorIdentityV1;
  provenance: EventProvenanceV1;
  guards: Array<{ name: string; decision: 'allow' | 'ask' | 'block'; reason?: string }>;
  effectiveDecision: 'allow' | 'ask' | 'block';
  authorizationReceiptId?: string;
  createdAt: string;
  outputReview?: { status: 'not-required' | 'passed' | 'blocked'; reason?: string };
}

const record = (value: unknown, name: string): Record<string, unknown> => {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${name} must be an object`);
  return value as Record<string, unknown>;
};
const text = (value: unknown, name: string): string => {
  if (typeof value !== 'string' || !value.trim()) throw new Error(`${name} is required`);
  return value;
};
const iso = (value: unknown, name: string): string => {
  const result = text(value, name);
  if (!Number.isFinite(Date.parse(result))) throw new Error(`${name} must be an ISO timestamp`);
  return result;
};

function parseActor(value: unknown): ActorIdentityV1 {
  const input = record(value, 'actor');
  const kind = text(input.kind, 'actor.kind') as ActorKind;
  if (!(ACTOR_KINDS as readonly string[]).includes(kind)) throw new Error('actor.kind is invalid');
  return { kind, id: text(input.id, 'actor.id') };
}

function parseProvenance(value: unknown, actor: ActorIdentityV1): EventProvenanceV1 {
  const input = record(value, 'provenance');
  const source = text(input.source, 'provenance.source') as ProvenanceSource;
  const trust = text(input.trust, 'provenance.trust') as TrustClass;
  if (!(PROVENANCE_SOURCES as readonly string[]).includes(source)) throw new Error('provenance.source is invalid');
  if (!(TRUST_CLASSES as readonly string[]).includes(trust)) throw new Error('provenance.trust is invalid');
  if (actor.kind === 'system' && source !== 'harness') throw new Error(`system actor cannot originate from ${source}`);
  if (source !== 'harness' && source !== 'session-operator' && trust === 'authority') {
    throw new Error(`${source} provenance cannot select authority trust`);
  }
  return { source, trust };
}

export function parseAgentEventEnvelopeV1<T = unknown>(value: unknown): AgentEventEnvelopeV1<T> {
  const input = record(value, 'event');
  if (input.version !== 1) throw new Error('event.version must be 1');
  const actor = parseActor(input.actor);
  const provenance = parseProvenance(input.provenance, actor);
  const aggregateInput = input.aggregate === undefined ? undefined : record(input.aggregate, 'aggregate');
  return {
    version: 1,
    eventId: text(input.eventId, 'eventId'),
    workspace: text(input.workspace, 'workspace'),
    ...(input.sessionId === undefined ? {} : { sessionId: text(input.sessionId, 'sessionId') }),
    ...(input.correlationId === undefined ? {} : { correlationId: text(input.correlationId, 'correlationId') }),
    type: text(input.type, 'type'),
    actor,
    provenance,
    ...(aggregateInput ? { aggregate: {
      kind: text(aggregateInput.kind, 'aggregate.kind'),
      id: text(aggregateInput.id, 'aggregate.id'),
      ...(aggregateInput.revision === undefined ? {} : { revision: text(aggregateInput.revision, 'aggregate.revision') }),
    } } : {}),
    createdAt: iso(input.createdAt, 'createdAt'),
    ...(input.expiresAt === undefined ? {} : { expiresAt: iso(input.expiresAt, 'expiresAt') }),
    payload: input.payload as T,
  };
}

export function parseAuthorizationReceiptV1(value: unknown): AuthorizationReceiptV1 {
  const input = record(value, 'authorization receipt');
  if (input.version !== 1) throw new Error('authorization.version must be 1');
  const actor = parseActor(input.actor);
  const provenance = parseProvenance(input.provenance, actor);
  if (actor.kind !== 'user' || provenance.source !== 'session-operator' || provenance.trust !== 'authority') {
    throw new Error('authorization receipts require a session-operator user actor');
  }
  if (!Array.isArray(input.scope) || input.scope.length === 0) throw new Error('scope is required');
  return {
    version: 1,
    receiptId: text(input.receiptId, 'receiptId'),
    interactionId: text(input.interactionId, 'interactionId'),
    workspace: text(input.workspace, 'workspace'),
    sessionId: text(input.sessionId, 'sessionId'),
    planId: text(input.planId, 'planId'),
    revision: text(input.revision, 'revision'),
    scope: input.scope.map((entry, index) => text(entry, `scope[${index}]`)),
    actor,
    provenance,
    createdAt: iso(input.createdAt, 'createdAt'),
    ...(input.expiresAt === undefined ? {} : { expiresAt: iso(input.expiresAt, 'expiresAt') }),
    ...(input.consumedAt === undefined ? {} : { consumedAt: iso(input.consumedAt, 'consumedAt') }),
  };
}

export function parseInteractionRequestV1(value: unknown): InteractionRequestV1 {
  const input = record(value, 'interaction request');
  if (input.version !== 1) throw new Error('interaction.version must be 1');
  if (input.kind !== 'question' && input.kind !== 'authorization') throw new Error('interaction.kind is invalid');
  if (input.status !== 'pending') throw new Error('new interaction status must be pending');
  if (!Array.isArray(input.options) || input.options.length === 0) throw new Error('interaction options are required');
  const options = input.options.map((option, index) => {
    const candidate = record(option, `options[${index}]`);
    return {
      id: text(candidate.id, `options[${index}].id`),
      label: text(candidate.label, `options[${index}].label`),
      ...(candidate.description === undefined ? {} : { description: text(candidate.description, `options[${index}].description`) }),
      ...(candidate.recommended === undefined ? {} : { recommended: candidate.recommended === true }),
      ...(candidate.disabledReason === undefined ? {} : { disabledReason: text(candidate.disabledReason, `options[${index}].disabledReason`) }),
    };
  });
  if (new Set(options.map((option) => option.id)).size !== options.length) throw new Error('interaction option IDs must be unique');
  return {
    version: 1,
    interactionId: text(input.interactionId, 'interactionId'),
    workspace: text(input.workspace, 'workspace'),
    sessionId: text(input.sessionId, 'sessionId'),
    correlationId: text(input.correlationId, 'correlationId'),
    kind: input.kind,
    question: text(input.question, 'question'),
    options,
    status: 'pending',
    createdAt: iso(input.createdAt, 'createdAt'),
    ...(input.expiresAt === undefined ? {} : { expiresAt: iso(input.expiresAt, 'expiresAt') }),
  };
}

export function parseInteractionAnswerV1(value: unknown): InteractionAnswerV1 {
  const input = record(value, 'interaction answer');
  if (input.version !== 1) throw new Error('interaction answer version must be 1');
  const actor = parseActor(input.actor);
  const provenance = parseProvenance(input.provenance, actor);
  if (actor.kind !== 'user' || provenance.source !== 'session-operator') throw new Error('interaction answers require the session operator');
  const optionIds = input.optionIds === undefined ? undefined : (() => {
    if (!Array.isArray(input.optionIds)) throw new Error('optionIds must be an array');
    return input.optionIds.map((entry, index) => text(entry, `optionIds[${index}]`));
  })();
  const cancelled = input.cancelled === true;
  const answerText = input.text === undefined ? undefined : text(input.text, 'text');
  if (!cancelled && !answerText && !optionIds?.length) throw new Error('answer requires optionIds, text, or cancellation');
  return {
    version: 1,
    interactionId: text(input.interactionId, 'interactionId'),
    correlationId: text(input.correlationId, 'correlationId'),
    sessionId: text(input.sessionId, 'sessionId'),
    actor,
    provenance,
    ...(optionIds ? { optionIds } : {}),
    ...(answerText ? { text: answerText } : {}),
    ...(cancelled ? { cancelled: true } : {}),
    createdAt: iso(input.createdAt, 'createdAt'),
  };
}

export function assertContextSegmentAuthority(segment: ContextSegmentV1): ContextSegmentV1 {
  if (segment.version !== 1) throw new Error('context segment version must be 1');
  if (!segment.id || !segment.origin || !segment.digest) throw new Error('context segment identity is incomplete');
  if (['peer-event', 'memory-lead', 'tool-result'].includes(segment.kind) && segment.authority !== 'external-data') {
    throw new Error(`${segment.kind} must use external-data authority`);
  }
  if (segment.kind === 'tool-contract' && segment.authority === 'product' && segment.origin !== 'octocode-harness') {
    throw new Error('external tool contracts cannot select product authority');
  }
  return segment;
}

export function contentDigest(content: string): string {
  return `sha256:${createHash('sha256').update(content).digest('hex')}`;
}

export function effectiveCapabilityDecision(guards: CapabilityDecisionReceiptV1['guards']): 'allow' | 'ask' | 'block' {
  if (guards.some((guard) => guard.decision === 'block')) return 'block';
  if (guards.some((guard) => guard.decision === 'ask')) return 'ask';
  return 'allow';
}

const PROPOSAL_TOPICS = new Set(['DECISION', 'PROPOSAL', 'APPROVAL', 'REQUEST']);
const BLOCKING_TOPICS = new Set(['BLOCKED', 'OVERLAP', 'CONFLICT']);
const HANDOFF_TOPICS = new Set(['HANDOFF']);

/**
 * Classify peer input without interpreting its body as host policy. Topic is the
 * stable routing field; text heuristics only provide a conservative fallback for
 * older senders that omitted it.
 */
export function classifyPeerMessage(topic: string | null | undefined, body: string): PeerMessageClass {
  const normalizedTopic = topic?.trim().toUpperCase() ?? '';
  if (PROPOSAL_TOPICS.has(normalizedTopic)) return 'proposal';
  if (BLOCKING_TOPICS.has(normalizedTopic)) return 'blocking';
  if (HANDOFF_TOPICS.has(normalizedTopic)) return 'handoff';
  if (/\b(approve|authorize|permission|choose|decision needed)\b/i.test(body)) return 'proposal';
  if (/\b(blocked|conflict|overlap|cannot continue)\b/i.test(body)) return 'blocking';
  return 'informational';
}

/** Fail-closed inbound policy. A held proposal is visible to the operator but its
 * body is not delivered as model context until a human-mediated interaction
 * resolves it. No peer input can mint an authorization receipt. */
export function evaluatePeerInbound(input: {
  fromAgentId: string;
  toAgentId?: string | null;
  expectedAgentId: string;
  topic?: string | null;
  text: string;
  maxBytes?: number;
}): PeerInboundPolicyResultV1 {
  const from = input.fromAgentId.trim();
  const expected = input.expectedAgentId.trim();
  const to = input.toAgentId?.trim() || null;
  const body = input.text.trim();
  const messageClass = classifyPeerMessage(input.topic, body);
  if (!from || !expected || !body) return { version: 1, decision: 'refuse', messageClass, reason: 'missing peer identity, target, or body' };
  if (from === expected) return { version: 1, decision: 'refuse', messageClass, reason: 'self-authored messages are not inbound peer events' };
  if (to !== null && to !== expected) return { version: 1, decision: 'refuse', messageClass, reason: 'message target does not match this agent' };
  if (Buffer.byteLength(body, 'utf8') > (input.maxBytes ?? 16_384)) {
    return { version: 1, decision: 'refuse', messageClass, reason: 'message exceeds the inbound size limit' };
  }
  if (messageClass === 'proposal') {
    return { version: 1, decision: 'hold', messageClass, reason: 'peer proposals require a human-mediated decision' };
  }
  return {
    version: 1,
    decision: 'accept',
    messageClass,
    reason: 'attributed peer data accepted',
    attributedText: `[peer:${from}; class:${messageClass}; authority:data]\n${body}`,
  };
}
