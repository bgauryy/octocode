import { createHash } from 'node:crypto';
import path from 'node:path';
import {
  AWARENESS_PEER_EVENT_MESSAGE_TYPE,
  createAwarenessEventConsumer,
  type AwarenessEventObservability,
  type AwarenessEventStore,
  type AwarenessPeerDelivery,
} from '@octocodeai/octocode-awareness';
import type { PiContext, PiInstance } from '../types.js';

export { AWARENESS_PEER_EVENT_MESSAGE_TYPE, createAwarenessEventConsumer };
export type { AwarenessEventObservability, AwarenessEventStore, AwarenessPeerDelivery };

/** Render only current-drain delivery pressure; lifetime totals are diagnostic history. */
export function awarenessEventStatusText(stats: AwarenessEventObservability): string | undefined {
  const attention = stats.backlogDepth > 0
    || stats.drainHeld > 0
    || stats.drainRefused > 0
    || stats.drainErrors > 0;
  if (!attention) return undefined;
  return `events q ${stats.backlogDepth}${stats.backlogCapped ? '+' : ''} · ack ${stats.lastAcknowledgedSequence} · accepted ${stats.drainAccepted} · held ${stats.drainHeld} · refused ${stats.drainRefused} · errors ${stats.drainErrors}`;
}

interface RegisterAwarenessEventConsumerOptions {
  openStore?: (workspace: string) => AwarenessEventStore;
  resolveExpectedAgentId?(ctx: PiContext): string;
  onObservability?(stats: AwarenessEventObservability, ctx: PiContext): void;
  now?: () => number;
  maxEventsPerDrain?: number;
  onDelivery?(message: AwarenessPeerDelivery, ctx: PiContext): void;
}

const nonEmptyString = (value: unknown): string | undefined => (
  typeof value === 'string' && value.trim() ? value.trim() : undefined
);

function isPersistedPeerDelivery(entry: unknown, message: AwarenessPeerDelivery): boolean {
  if (!entry || typeof entry !== 'object') return false;
  const record = entry as Record<string, unknown>;
  if (record['type'] !== 'custom_message' || record['customType'] !== message.customType) return false;
  const details = record['details'];
  if (!details || typeof details !== 'object') return false;
  const receipt = details as Record<string, unknown>;
  return receipt['eventId'] === message.details.eventId
    && receipt['sequence'] === message.details.sequence;
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

export function resolvePiEventConsumerId(ctx: PiContext): string | undefined {
  const sessionId = nonEmptyString(ctx.sessionManager?.getSessionId?.());
  const sessionFile = nonEmptyString(ctx.sessionManager?.getSessionFile?.());
  if (sessionId) return `pi:${sessionId}`;
  if (!sessionFile) return undefined;
  const normalized = path.normalize(path.resolve(sessionFile));
  return `pi:file:${createHash('sha256').update(normalized).digest('hex').slice(0, 24)}`;
}

/** Register event-driven wake points only; there is deliberately no polling loop. */
export function registerAwarenessEventConsumer(pi: PiInstance, options: RegisterAwarenessEventConsumerOptions = {}): void {
  const consumers = new Map<string, ReturnType<typeof createAwarenessEventConsumer>>();
  const drain = async (ctx: PiContext): Promise<void> => {
    const workspace = path.resolve(ctx.cwd ?? process.cwd());
    const consumerId = resolvePiEventConsumerId(ctx);
    if (!consumerId) {
      options.onObservability?.({ ...initialObservability('unavailable'), errors: 1, drainErrors: 1 }, ctx);
      return;
    }
    const expectedAgentId = options.resolveExpectedAgentId?.(ctx);
    if (!expectedAgentId?.trim()) {
      options.onObservability?.({ ...initialObservability(consumerId), errors: 1, drainErrors: 1 }, ctx);
      return;
    }
    const key = `${workspace}\0${consumerId}`;
    let consumer = consumers.get(key);
    if (!consumer) {
      consumer = createAwarenessEventConsumer({
        workspace,
        consumerId,
        expectedAgentId,
        ...(options.openStore ? { openStore: options.openStore } : {}),
        ...(options.now ? { now: options.now } : {}),
        ...(options.maxEventsPerDrain ? { maxEventsPerDrain: options.maxEventsPerDrain } : {}),
        deliver: (message) => {
          if (!pi.sendMessage) throw new Error('Pi custom message delivery is unavailable');
          const readEntries = ctx.sessionManager?.getEntries ?? ctx.sessionManager?.getBranch;
          if (!readEntries) {
            throw new Error('Pi session persistence receipts are unavailable; Awareness event remains unacknowledged');
          }

          /*
           * Pi 0.84.x sendMessage returns void and its runtime swallows asynchronous
           * sendCustomMessage rejection. Avoid nextTurn (which is only an in-memory
           * queue) and positively verify the event identity in the durable session
           * ledger before allowing Awareness to advance its cursor.
           */
          const persisted = () => readEntries.call(ctx.sessionManager)
            .some((entry) => isPersistedPeerDelivery(entry, message));
          if (!persisted()) pi.sendMessage(message, { triggerTurn: false });
          if (!persisted()) {
            throw new Error('Pi custom message persistence was not confirmed; Awareness event remains unacknowledged');
          }
          options.onDelivery?.(message, ctx);
        },
        onObservability: (stats) => options.onObservability?.(stats, ctx),
      });
      consumers.set(key, consumer);
    }
    await consumer.drain();
  };

  pi.on('session_start', async (_event, ctx) => { await drain(ctx); });
  pi.on('turn_end', async (_event, ctx) => { await drain(ctx); });
}
