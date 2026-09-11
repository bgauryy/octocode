import type { PiContext, PiInstance } from '../types.js';
import {
  readSessionPeerEvent,
  registerCurrentContextSource,
  sessionPeerEventOrigin,
} from './context-source-registry.js';
import { estimateContextTokens } from './context-segments.js';
import {
  awarenessEventStatusText,
  registerAwarenessEventConsumer,
} from './awareness-event-consumer.js';
import { getAwarenessAgentId } from './awareness-shared.js';
import {
  clearAwarenessCacheEntry,
  refreshAwarenessPanel,
} from './awareness-status.js';
import { runtimeStoreFor } from './runtime-renderer.js';

export interface AwarenessRuntimeRegistrationOptions {
  refreshUi(ctx: PiContext): void;
}

/** Own Pi's Awareness delivery, prompt-source, cache, and observability wiring. */
export function registerAwarenessRuntime(
  pi: PiInstance,
  options: AwarenessRuntimeRegistrationOptions
): void {
  registerAwarenessEventConsumer(pi, {
    resolveExpectedAgentId: ctx => getAwarenessAgentId(ctx),
    onDelivery: (message, ctx) => {
      const eventId = message.details.eventId;
      runtimeStoreFor(ctx)?.getState().setContext({
        lastPeerDeliveryEstimate: {
          method: 'ceil-utf16-chars/4',
          sequence: message.details.sequence,
          tokens: estimateContextTokens(message.content),
        },
      });
      registerCurrentContextSource(ctx, {
        version: 1,
        id: `peer-event:${eventId}`,
        kind: 'peer-event',
        origin: sessionPeerEventOrigin(eventId),
        authority: 'external-data',
        scope: 'turn',
        visibility: 'inspectable',
        rehydrate: 'on-trigger',
        capture: false,
        readCurrent: current => readSessionPeerEvent(current, eventId),
      });
    },
    onObservability: (stats, ctx) => {
      runtimeStoreFor(ctx)?.getState().setStatus(
        'octocode-awareness-events',
        awarenessEventStatusText(stats)
      );
      if (stats.drainAccepted > 0) {
        clearAwarenessCacheEntry(ctx.cwd ?? process.cwd());
        refreshAwarenessPanel(ctx);
      }
      options.refreshUi(ctx);
    },
  });
}
