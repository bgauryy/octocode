import type { AwarenessClientContext } from './client.js';
import { connectDb, resolveDbPath } from './db-runtime.js';
import { runAwarenessHistoryOperation } from './history-api.js';
import type { HistoryCaptureInput } from './schema/definitions-history.js';
import { storageScopeForOperation } from './workspace-policy.js';

type BindHistoryCaptureContext<T> = T extends unknown
  ? Omit<T, 'workspace' | 'agent_id'>
  : never;

export type AwarenessHistoryCaptureInput = BindHistoryCaptureContext<HistoryCaptureInput>;

export interface AwarenessHost {
  readonly context: Readonly<AwarenessClientContext>;
  captureHistory(input: AwarenessHistoryCaptureInput): Promise<Record<string, unknown>>;
}

/** Host-only lifecycle capabilities that are deliberately absent from model discovery. */
export function createAwarenessHost(context: AwarenessClientContext): AwarenessHost {
  const bound = Object.freeze({ ...context });
  return Object.freeze({
    context: bound,
    async captureHistory(input: AwarenessHistoryCaptureInput): Promise<Record<string, unknown>> {
      const scope = storageScopeForOperation('host.history.capture', bound.workspace, bound.scope);
      const database = connectDb(resolveDbPath(bound.database, { workspace: bound.workspace, scope }));
      try {
        return await runAwarenessHistoryOperation(database, 'capture', {
          ...input,
          workspace: bound.workspace,
          agent_id: bound.agentId,
          ...(bound.sessionId && input.session_id === undefined ? { session_id: bound.sessionId } : {}),
        });
      } finally {
        database.close();
      }
    },
  });
}

export { historyToolEffect } from './history-tool-effects.js';
export type { HistoryToolEffect } from './history-tool-effects.js';
export {
  contentDigest,
  assertContextSegmentAuthority,
  effectiveCapabilityDecision,
} from './continuity-contracts.js';
export type {
  AuthorizationReceiptV1,
  CapabilityDecisionReceiptV1,
  ContextSegmentV1,
  InboundDecision,
  InteractionAnswerV1,
  InteractionRequestV1,
} from './continuity-contracts.js';
export {
  AWARENESS_PEER_EVENT_MESSAGE_TYPE,
  createAwarenessEventConsumer,
  createAwarenessEventObservability,
} from './event-consumer.js';
export type {
  AwarenessEventConsumerOptions,
  AwarenessEventObservability,
  AwarenessEventStore,
  AwarenessPeerDelivery,
} from './event-consumer.js';
export { watchAwarenessEventHints } from './event-wake-hints.js';
export type { AwarenessEventHintOptions } from './event-wake-hints.js';
export { connectDb } from './db-runtime.js';
export { normalizeWorkspacePath } from './git.js';
export {
  AWARENESS_PI_HOST_PROMPT,
  formatExternalAgentAwarenessInstructions,
  formatExternalAgentCoordinationContext,
  getExternalAgentAwarenessGuide,
} from './coordination/external-policy.js';
export { openAwarenessStore } from './coordination/open.js';
export { runPreEditLockGate } from './coordination/hooks.js';
export type { PreEditHookOptions, PreEditHookResult } from './coordination/hooks.js';
export { readExternalAwarenessStatus } from './coordination/external-status.js';
export type { ExternalAwarenessStatus } from './coordination/external-status.js';
export { completeExternalPlanTask, finalizeExternalPlan, projectExternalPlan } from './coordination/external-plan.js';
export type { ExternalPlanScope, ObservedCheckReceipt } from './coordination/external-plan.js';
export { detectAgentHost, generateAgentName } from './coordination/agent-naming.js';
export type { AgentHost } from './coordination/agent-naming.js';
export type { OutboxEventV1, StoredInteractionV1 } from './coordination/coordination-continuity.js';
export { assessRuntimeRegulation } from './attend-physiology.js';
export { defaultDbPath } from './coordination/coordination-shared.js';
export { claimNativeHookOwner, loadWorkspacePolicy, storageScopeForOperation } from './workspace-policy.js';
export type { AwarenessIntegrationHost, AwarenessHookOwner } from './workspace-policy.js';
export type { AwarenessStorageScope } from './storage-scope.js';
