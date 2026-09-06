import {
  evaluatePeerInbound,
  parseAgentEventEnvelopeV1,
  type InboundDecision,
} from './continuity-contracts.js';
import { openAwarenessStore } from './coordination/open.js';
import type { OutboxEventV1 } from './coordination/coordination-continuity.js';
import { normalizeWorkspacePath } from './git.js';

export const AWARENESS_PEER_EVENT_MESSAGE_TYPE = 'octocode-peer-event';

export interface AwarenessEventStore {
  listEvents(params: { consumerId: string; limit?: number }): OutboxEventV1[];
  acknowledgeEvent(params: { consumerId: string; eventId: string; decision: InboundDecision }): {
    sequence: number;
    decision: InboundDecision;
    duplicate: boolean;
  };
  getConsumerCursor(consumerId: string): number;
  markMessageRead(params: { messageId: string; agentId: string }): unknown;
  close(): void;
}

export interface AwarenessEventObservability {
  consumerId: string;
  backlogDepth: number;
  backlogCapped: boolean;
  lastAcknowledgedSequence: number;
  accepted: number;
  held: number;
  refused: number;
  errors: number;
  /** Outcomes from only the most recent drain; lifetime counters remain above. */
  drainAccepted: number;
  drainHeld: number;
  drainRefused: number;
  drainErrors: number;
}

export interface AwarenessPeerDelivery {
  customType: typeof AWARENESS_PEER_EVENT_MESSAGE_TYPE;
  content: string;
  display: false;
  details: {
    version: 1;
    eventId: string;
    sequence: number;
    createdAt: string;
    messageClass: 'informational' | 'blocking' | 'handoff';
    provenance: 'peer-attributed-data';
  };
}

export interface AwarenessEventConsumerOptions {
  workspace: string;
  consumerId: string;
  expectedAgentId: string;
  openStore?: (workspace: string) => AwarenessEventStore;
  deliver(message: AwarenessPeerDelivery): void | InboundDecision | Promise<void | InboundDecision>;
  onObservability?(stats: AwarenessEventObservability): void;
  now?: () => number;
  maxEventsPerDrain?: number;
}

interface PeerMessagePayload {
  messageId: string;
  fromAgentId: string;
  toAgentId: string | null;
  topic: string | null;
  text: string;
}

const nonEmptyString = (value: unknown): string | undefined => (
  typeof value === 'string' && value.trim() ? value.trim() : undefined
);

function parsePeerPayload(event: ReturnType<typeof parseAgentEventEnvelopeV1>): PeerMessagePayload {
  if (event.type !== 'peer.message') throw new Error('event is not a peer message');
  if (event.actor.kind !== 'agent' || event.provenance.source !== 'peer' || event.provenance.trust !== 'attributed-data') {
    throw new Error('peer message provenance is invalid');
  }
  if (!event.payload || typeof event.payload !== 'object' || Array.isArray(event.payload)) throw new Error('peer message payload is invalid');
  const payload = event.payload as Record<string, unknown>;
  const messageId = nonEmptyString(payload['messageId']);
  const fromAgentId = nonEmptyString(payload['fromAgentId']);
  const text = nonEmptyString(payload['text']);
  if (!messageId || !fromAgentId || !text) throw new Error('peer message identity and body are required');
  if (event.actor.id !== fromAgentId) throw new Error('peer message actor does not match its payload');
  if (event.aggregate && (event.aggregate.kind !== 'message' || event.aggregate.id !== messageId)) {
    throw new Error('peer message aggregate does not match its payload');
  }
  return {
    messageId,
    fromAgentId,
    toAgentId: nonEmptyString(payload['toAgentId']) ?? null,
    topic: nonEmptyString(payload['topic']) ?? null,
    text,
  };
}

const initialObservability = (consumerId: string): AwarenessEventObservability => ({
  consumerId,
  backlogDepth: 0,
  backlogCapped: false,
  lastAcknowledgedSequence: 0,
  accepted: 0,
  held: 0,
  refused: 0,
  errors: 0,
  drainAccepted: 0,
  drainHeld: 0,
  drainRefused: 0,
  drainErrors: 0,
});

/** Bounded serialized transaction-outbox consumer for attributed peer data. */
export function createAwarenessEventConsumer(options: AwarenessEventConsumerOptions) {
  const openStore = options.openStore ?? ((workspace: string) => openAwarenessStore({ workspace }));
  const workspace = normalizeWorkspacePath(options.workspace, options.workspace) ?? options.workspace;
  const maxEvents = Math.min(Math.max(options.maxEventsPerDrain ?? 100, 1), 999);
  const now = options.now ?? Date.now;
  const stats = initialObservability(options.consumerId);
  let inFlight: Promise<AwarenessEventObservability> | undefined;

  const drainOnce = async (): Promise<AwarenessEventObservability> => {
    stats.drainAccepted = 0;
    stats.drainHeld = 0;
    stats.drainRefused = 0;
    stats.drainErrors = 0;
    let store: AwarenessEventStore | undefined;
    try {
      store = openStore(workspace);
      stats.lastAcknowledgedSequence = store.getConsumerCursor(options.consumerId);
      const pending = store.listEvents({ consumerId: options.consumerId, limit: maxEvents + 1 });
      for (const candidate of pending.slice(0, maxEvents)) {
        let decision: InboundDecision = 'refuse';
        let delivery: AwarenessPeerDelivery | undefined;
        let peerMessage: PeerMessagePayload | undefined;
        try {
          const event = parseAgentEventEnvelopeV1(candidate);
          if (event.workspace !== workspace) throw new Error('event workspace does not match this consumer');
          if (event.expiresAt && Date.parse(event.expiresAt) <= now()) {
            decision = 'refuse';
          } else if (event.type === 'peer.message') {
            peerMessage = parsePeerPayload(event);
            const policy = evaluatePeerInbound({
              fromAgentId: peerMessage.fromAgentId,
              toAgentId: peerMessage.toAgentId,
              expectedAgentId: options.expectedAgentId,
              topic: peerMessage.topic,
              text: peerMessage.text,
            });
            decision = policy.decision;
            if (decision === 'accept' && policy.attributedText) {
              delivery = {
                customType: AWARENESS_PEER_EVENT_MESSAGE_TYPE,
                content: policy.attributedText,
                display: false,
                details: {
                  version: 1,
                  eventId: candidate.eventId,
                  sequence: candidate.sequence,
                  createdAt: candidate.createdAt,
                  messageClass: policy.messageClass as 'informational' | 'blocking' | 'handoff',
                  provenance: 'peer-attributed-data',
                },
              };
            }
          }
        } catch {
          stats.errors += 1;
          stats.drainErrors += 1;
          decision = 'refuse';
          delivery = undefined;
        }
        try {
          if (delivery) decision = (await options.deliver(delivery)) ?? decision;
          if (decision === 'accept' && peerMessage) {
            store.markMessageRead({ messageId: peerMessage.messageId, agentId: options.expectedAgentId });
          }
          const ack = store.acknowledgeEvent({ consumerId: options.consumerId, eventId: candidate.eventId, decision });
          stats.lastAcknowledgedSequence = ack.sequence;
          if (!ack.duplicate) {
            if (decision === 'accept') stats.accepted += 1;
            else if (decision === 'hold') stats.held += 1;
            else stats.refused += 1;
            if (decision === 'accept') stats.drainAccepted += 1;
            else if (decision === 'hold') stats.drainHeld += 1;
            else stats.drainRefused += 1;
          }
        } catch {
          stats.errors += 1;
          stats.drainErrors += 1;
          break;
        }
      }
      const remaining = store.listEvents({ consumerId: options.consumerId, limit: 1000 });
      stats.backlogDepth = remaining.length;
      stats.backlogCapped = remaining.length === 1000;
      options.onObservability?.({ ...stats });
      return { ...stats };
    } catch {
      stats.errors += 1;
      stats.drainErrors += 1;
      options.onObservability?.({ ...stats });
      return { ...stats };
    } finally {
      store?.close();
    }
  };

  return {
    drain(): Promise<AwarenessEventObservability> {
      if (inFlight) return inFlight;
      inFlight = drainOnce().finally(() => { inFlight = undefined; });
      return inFlight;
    },
    snapshot(): AwarenessEventObservability { return { ...stats }; },
  };
}
