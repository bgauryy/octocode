/**
 * Canonical routine Awareness API.
 *
 * Host lifecycle and operator capabilities are explicit subpaths:
 * `@octocodeai/octocode-awareness/host` and
 * `@octocodeai/octocode-awareness/admin`.
 */
export { createAwarenessClient } from './client.js';
export {
  AWARENESS_AGENT_INSTRUCTION_SECTIONS,
  AWARENESS_MESSAGE_PARAMETER_GUIDANCE,
  getAwarenessAgentInstructions,
} from './agent-instructions.js';
export type { AwarenessAgentInstructionSection } from './agent-instructions.js';
export type { ContextObservation, ContextFeedback, ContextAdvisory, ContextAdvisoryKind } from './context-regulation.js';
export type { RunState, ContextNudge } from './context-state.js';
export {
  AWARENESS_CONCEPTS,
  ROUTINE_AWARENESS_OPERATIONS,
  getAwarenessOperationDescriptor,
  listAwarenessOperationDescriptors,
} from './schema/operation-catalog.js';
export type {
  AwarenessClient,
  AwarenessClientContext,
  AwarenessEventCursor,
  AwarenessExecutableCall,
  AwarenessHostEventInput,
  AwarenessItemSummary,
  AwarenessOrientation,
  AwarenessOrientationResult,
  AwarenessOrientationUnchanged,
  AwarenessPeerSummary,
} from './client.js';
export type {
  AwarenessInsightCandidate,
  AwarenessInsightProvider,
  AwarenessOperationEffect,
  AwarenessOperationResult,
} from './operation-contracts.js';
export type {
  AwarenessConcept,
  AwarenessOperation,
  AwarenessOperationCall,
  AwarenessOperationDescriptor,
  AwarenessOperationParams,
} from './schema/operation-catalog.js';
