import { existsSync } from 'node:fs';
import path from 'node:path';
import { isPersistentStorageEnabledForExtension as isPersistentStorageEnabled } from '@octocodeai/config';
import {
  createAwarenessEventConsumer,
  watchAwarenessEventHints,
  type AwarenessEventObservability,
  type AwarenessEventStore,
  type AwarenessPeerDelivery,
} from '@octocodeai/octocode-awareness/host';
import type { PiContext, PiInstance } from '../types.js';
import { openPersistentAwareness } from './storage-policy.js';
import { notifyDesktopAttention } from './desktop-notify.js';
import { resolveAwarenessSessionAgentId } from './awareness-shared.js';

/** Render only current-drain delivery pressure; lifetime totals are diagnostic history. */
export function awarenessEventStatusText(stats: AwarenessEventObservability): string | undefined {
  const attention = stats.backlogDepth > 0
    || stats.drainHeld > 0
    || stats.drainRefused > 0
    || stats.drainErrors > 0;
  if (!attention) return undefined;
  // Show only the non-zero drain-pressure signals; omit zero-noise fields.
  const parts: string[] = [];
  if (stats.backlogDepth > 0) parts.push(`${stats.backlogDepth}${stats.backlogCapped ? '+' : ''} queued`);
  if (stats.drainErrors > 0) parts.push(`${stats.drainErrors} err`);
  if (stats.drainRefused > 0) parts.push(`${stats.drainRefused} refused`);
  if (stats.drainHeld > 0) parts.push(`${stats.drainHeld} held`);
  return `peer events · ${parts.length > 0 ? parts.join(' · ') : 'draining'} · seq ${stats.lastAcknowledgedSequence}`;
}

interface RegisterAwarenessEventConsumerOptions {
  openStore?: (workspace: string) => AwarenessEventStore;
  watchEvents?: typeof watchAwarenessEventHints;
  resolveExpectedAgentId?(ctx: PiContext): string;
  onObservability?(stats: AwarenessEventObservability, ctx: PiContext): void;
  now?: () => number;
  maxEventsPerDrain?: number;
  /** Host authorization gate; unknown project trust fails closed by default. */
  canWake?(ctx: PiContext): boolean;
  onDelivery?(message: AwarenessPeerDelivery, ctx: PiContext): void;
}

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

const registeredAwarenessHosts = new WeakSet<object>();

/** Lifecycle and coalesced database hints wake bounded drains without idle polling. */
export function registerAwarenessEventConsumer(pi: PiInstance, options: RegisterAwarenessEventConsumerOptions = {}): void {
  // The extension owns one consumer for the host instance. Guard repeated
  // registration during reload/setup so each event has one drain and one
  // acknowledgment path; the durable cursor still protects process restarts.
  const host = pi as unknown as object;
  if (registeredAwarenessHosts.has(host)) return;
  registeredAwarenessHosts.add(host);
  // Session-scoped: Pi creates a new ctx object on every agent_end turn, so a
  // WeakMap keyed on ctx would never hit across turns and the toast would fire
  // repeatedly. One string per consumer is sufficient — there is at most one
  // active drain at a time and it is reset on session_start.
  let lastAttention: string | undefined;
  let lastAttentionAt = 0;
  const observe = (stats: AwarenessEventObservability, ctx: PiContext): void => {
    options.onObservability?.(stats, ctx);
    // Queue churn is status, while held decisions and delivery failures need attention.
    const attention = stats.drainErrors > 0 ? 'Peer message delivery is unavailable; messages may be delayed.'
      : stats.drainHeld > 0 ? 'A peer proposal needs a human decision. Inspect the Awareness inbox.'
        : undefined;
    if (!attention) return; // don't clear dedup mid-retry; only session_start resets it
    const now = Date.now();
    if (lastAttention === attention && now - lastAttentionAt < 30_000) return;
    lastAttention = attention;
    lastAttentionAt = now;
    try { if (ctx.hasUI) ctx.ui?.notify?.(`Awareness: ${attention}`, 'warning'); } catch { /* stale UI cannot change delivery */ }
  };
  let generation = 0;
  let shutdown = false;
  let running = false;
  let wakeAvailable = true;
  let pendingActionable = 0;
  let eventWatcher: ReturnType<typeof watchAwarenessEventHints> | undefined;
  let watchedDatabase: string | undefined;
  let deliveryRetries = 0;
  const stopWatching = (): void => {
    eventWatcher?.close();
    eventWatcher = undefined;
    watchedDatabase = undefined;
  };
  const recordActionable = (message: AwarenessPeerDelivery, expectedAgentId: string): void => {
    if (message.details.actionable && message.details.toAgentId === expectedAgentId) {
      pendingActionable = Math.min(pendingActionable + 1, 999);
    }
  };
  let activeConsumer: {
    key: string;
    consumer: ReturnType<typeof createAwarenessEventConsumer>;
    binding: { ctx: PiContext };
  } | undefined;
  const drain = async (ctx: PiContext): Promise<void> => {
    if (shutdown || running) return;
    if (!options.openStore && !isPersistentStorageEnabled()) { stopWatching(); return; }
    if (!pi.sendMessage) return; // delivery not yet supported by this host; retry at next turn
    const workspace = path.resolve(ctx.cwd ?? process.cwd());
    const consumerId = resolveAwarenessSessionAgentId(ctx);
    // consumerId is undefined when both getSessionId() and getSessionFile() are
    // falsy — the same 'session not yet established' state that the existsSync
    // guard below handles silently. Return without error; delivery resumes once
    // Pi assigns a session identity.
    if (!consumerId) return;
    // Pi defers creating a new session file until its first assistant message.
    // getEntries() alone is only memory before that point (and in --no-session).
    // Leave shared events unread for the next durable turn rather than consume
    // messages that would disappear if the user closes the fresh session.
    const sessionFile = ctx.sessionManager?.getSessionFile?.();
    if (!sessionFile || !existsSync(sessionFile)) return;
    const expectedAgentId = options.resolveExpectedAgentId?.(ctx);
    // If not yet resolved, defer — same silent treatment as missing consumerId or session file.
    if (!expectedAgentId?.trim()) return;
    const key = `${workspace}\0${consumerId}\0${expectedAgentId}`;
    let scoped = activeConsumer;
    if (scoped?.key !== key) {
      stopWatching();
      const consumerGeneration = ++generation;
      const assertActive = (): void => {
        if (shutdown || consumerGeneration !== generation) {
          throw new Error('Awareness delivery interrupted by a session transition; event remains unacknowledged');
        }
      };
      const binding = { ctx };
      const awaitingReceipt = new Set<string>();
      const consumer = createAwarenessEventConsumer({
        workspace,
        consumerId,
        expectedAgentId,
        openStore: (targetWorkspace) => {
          const store = options.openStore ? options.openStore(targetWorkspace) : openPersistentAwareness({ workspace: targetWorkspace });
          if (store.dbPath && store.dbPath !== ':memory:' && watchedDatabase !== store.dbPath) {
            stopWatching();
            watchedDatabase = store.dbPath;
            eventWatcher = (options.watchEvents ?? watchAwarenessEventHints)({
              database: store.dbPath,
              onHint: () => {
                if (shutdown || generation !== consumerGeneration) return;
                deliveryRetries = 0;
                scheduleDrain(binding.ctx);
              },
              onError: () => {
                if (shutdown || generation !== consumerGeneration) return;
                const stats = consumer.snapshot();
                observe({ ...stats, errors: stats.errors + 1, drainErrors: stats.drainErrors + 1 }, binding.ctx);
              },
            });
          }
          return store;
        },
        ...(options.now ? { now: options.now } : {}),
        ...(options.maxEventsPerDrain ? { maxEventsPerDrain: options.maxEventsPerDrain } : {}),
        deliver: async (message) => {
          assertActive();
          const currentCtx = binding.ctx;
          if (!pi.sendMessage) throw new Error('Pi custom message delivery is unavailable');
          const readEntries = currentCtx.sessionManager?.getEntries ?? currentCtx.sessionManager?.getBranch;
          if (!readEntries) {
            throw new Error('Pi session persistence receipts are unavailable; Awareness event remains unacknowledged');
          }

          const persisted = () => readEntries.call(currentCtx.sessionManager)
            .some((entry) => isPersistedPeerDelivery(entry, message));

          // Idempotency guard: already in the ledger from a previous drain cycle.
          if (persisted()) {
            if (awaitingReceipt.delete(message.details.eventId)) recordActionable(message, expectedAgentId);
            options.onDelivery?.(message, currentCtx);
            return;
          }

          /*
           * pi.sendMessage() wraps the async sendCustomMessage() in a fire-and-forget
           * .catch(), returning void synchronously while the internal write is still in
           * flight. deliverAs:'steer' is explicit: after the agent_end hooks return,
           * pi appends immediately to the session ledger rather than queuing for the
           * next user prompt ('nextTurn' is only an in-memory queue and does not survive
           * compaction or session reload).
           *
           * Two microtask yields give sendCustomMessage time to complete its internal
           * awaits before we verify persistence in the session ledger. The yields are
           * no-ops when the write is synchronous (test harness path) and necessary when
           * it is async (production sendCustomMessage implementation).
           */
          awaitingReceipt.add(message.details.eventId);
          pi.sendMessage({ ...message, display: true }, { triggerTurn: false, deliverAs: 'steer' });
          await Promise.resolve(); // let sendCustomMessage start executing
          await Promise.resolve(); // let its first internal await settle

          assertActive();
          if (persisted()) {
            if (awaitingReceipt.delete(message.details.eventId)) recordActionable(message, expectedAgentId);
            options.onDelivery?.(message, currentCtx);
            try {
              if (currentCtx.hasUI && message.details.messageClass !== 'informational') {
                const blocking = message.details.messageClass === 'blocking';
                currentCtx.ui?.notify?.(
                  `Awareness: ${blocking ? 'blocking message' : 'handoff'} received. See the peer card for details.`,
                  blocking ? 'warning' : 'info',
                );
                notifyDesktopAttention(currentCtx, `Octocode Awareness: ${blocking ? 'blocking message' : 'handoff'} received.`);
              }
            } catch { /* a failed toast must never cause peer-message replay */ }
            return;
          }

          throw new Error(
            `Pi custom message persistence was not confirmed for awareness event ` +
            `${message.details.eventId}; Awareness event remains unacknowledged`,
          );
        },
        onObservability: (stats) => {
          if (!shutdown && consumerGeneration === generation) observe(stats, binding.ctx);
        },
      });
      scoped = { key, consumer, binding };
      activeConsumer = scoped;
    }
    scoped.binding.ctx = ctx;
    const stats = await scoped.consumer.drain();
    if (shutdown || activeConsumer !== scoped) return;
    if (stats.drainErrors > 0) {
      const delay = [100, 500, 2000][deliveryRetries++];
      if (delay !== undefined) scheduleRetry(ctx, delay);
    } else if (stats.backlogDepth > 0) {
      deliveryRetries = 0;
      scheduleDrain(ctx);
    }
    if (shutdown || running || activeConsumer !== scoped || stats.backlogDepth > 0 || pendingActionable === 0) return;
    if (!wakeAvailable || !(options.canWake?.(ctx) ?? ctx.isProjectTrusted?.() === true)) {
      try { if (ctx.hasUI) ctx.ui?.notify?.('Awareness: actionable peer messages are in context; automatic wake is held until the next authorized input.', 'warning'); } catch { /* UI is advisory */ }
      return;
    }
    const count = pendingActionable;
    pendingActionable = 0;
    wakeAvailable = false;
    // One host turn for the complete persisted batch, without duplicating peer bodies.
    // Re-arm only on external input: peer ping-pong cannot recursively spend turns.
    try {
      if (!pi.sendMessage) throw new Error('Pi automatic wake delivery is unavailable');
      pi.sendMessage({ customType: 'octocode-peer-wake', content: `Awareness: ${count} actionable peer messages were delivered. Inspect the attributed peer context and coordinate the next safe action.`, display: false }, { triggerTurn: true, deliverAs: 'followUp' });
    } catch {
      // The peer receipt remains durable. Do not loop retrying a failed host wake.
      pendingActionable = count;
      try {
        const stats = scoped.consumer.snapshot();
        options.onObservability?.({ ...stats, errors: stats.errors + 1 }, ctx);
      } catch { /* diagnostics cannot change durable delivery */ }
      try {
        if (ctx.hasUI) ctx.ui?.notify?.(
          'Awareness: peer messages were delivered, but automatic wake-up is unavailable. Continue on the next authorized input.',
          'warning',
        );
      } catch { /* UI is advisory */ }
    }
  };

  let pendingDrain: ReturnType<typeof setImmediate> | undefined;
  let retryDrain: ReturnType<typeof setTimeout> | undefined;
  const scheduleDrain = (ctx: PiContext): void => {
    if (shutdown || running || pendingDrain) return;
    pendingDrain = setImmediate(() => {
      pendingDrain = undefined;
      void drain(ctx).catch(() => { /* observability callbacks must not crash the host */ });
    });
    pendingDrain.unref();
  };
  const scheduleRetry = (ctx: PiContext, delay: number): void => {
    if (shutdown || running || retryDrain) return;
    retryDrain = setTimeout(() => { retryDrain = undefined; scheduleDrain(ctx); }, delay);
    retryDrain.unref();
  };
  const cancelPendingDrain = (): void => {
    if (pendingDrain) clearImmediate(pendingDrain);
    pendingDrain = undefined;
    if (retryDrain) clearTimeout(retryDrain);
    retryDrain = undefined;
  };
  pi.on('session_start', async (_event, ctx) => {
    cancelPendingDrain();
    stopWatching();
    generation += 1;
    activeConsumer = undefined;
    shutdown = false;
    running = false;
    pendingActionable = 0;
    wakeAvailable = true;
    deliveryRetries = 0;
    lastAttention = undefined;
    lastAttentionAt = 0;
    await drain(ctx);
  });
  // Pi remains streaming until its agent_end hooks return. Drain on the next
  // event-loop turn so sendMessage persists immediately instead of queuing a
  // steer that cannot be observed while this hook is still executing.
  pi.on('agent_start', async () => { running = true; pendingActionable = 0; cancelPendingDrain(); });
  pi.on('input', async (event) => {
    if (event.source === 'interactive' || event.source === 'rpc') wakeAvailable = true;
  });
  pi.on('agent_end', async (event, ctx) => {
    cancelPendingDrain();
    if (shutdown || event.willRetry) return;
    running = false;
    deliveryRetries = 0;
    scheduleDrain(ctx);
  });
  pi.on('session_shutdown', async () => {
    cancelPendingDrain();
    stopWatching();
    shutdown = true;
    generation += 1;
    activeConsumer = undefined;
    pendingActionable = 0;
  });
}
