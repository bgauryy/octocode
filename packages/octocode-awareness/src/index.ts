/**
 * @octocodeai/octocode-awareness — public module API.
 *
 * Import directly — no subprocess required:
 *   import { getMemory, insertMemory, reflect } from '@octocodeai/octocode-awareness';
 */

// DB layer
export {
  connectDb, connectCachedDb, resolveDbPath, getDeliveryFingerprint, setDeliveryFingerprint,
} from './db-runtime.js';
export { initDb } from './db-init.js';
export { tableColumns } from './db-introspection.js';
export { hasFts, replaceMemoryReferences, referenceKind, evictExpiredLocks } from './db-maintenance.js';
export type { DeliveryFingerprintKey } from './db-runtime.js';
export {
  AWARENESS_CONFIG_QUESTIONS,
  AWARENESS_CONFIG_VERSION,
  DEFAULT_AWARENESS_CONFIG,
  awarenessConfigPath,
  awarenessFeatureEnabled,
  loadAwarenessConfig,
  parseAwarenessConfig,
  writeAwarenessConfig,
} from './awareness-config.js';
export type { AwarenessConfig, AwarenessFeatureConfig } from './awareness-config.js';

// Cross-boundary continuity contracts. These are dependency-free domain types
// and validators; durable storage remains owned by Awareness adapters below.
export {
  ACTOR_KINDS,
  PROVENANCE_SOURCES,
  TRUST_CLASSES,
  INBOUND_DECISIONS,
  parseAgentEventEnvelopeV1,
  parseAuthorizationReceiptV1,
  parseInteractionRequestV1,
  parseInteractionAnswerV1,
  assertContextSegmentAuthority,
  contentDigest,
  effectiveCapabilityDecision,
  classifyPeerMessage,
  evaluatePeerInbound,
} from './continuity-contracts.js';
export type {
  ActorKind,
  ProvenanceSource,
  TrustClass,
  InboundDecision,
  PeerMessageClass,
  PeerInboundPolicyResultV1,
  ActorIdentityV1,
  EventProvenanceV1,
  AgentEventEnvelopeV1,
  AuthorizationReceiptV1,
  InteractionRequestV1,
  InteractionAnswerV1,
  ContextSegmentV1,
  CapabilityDecisionReceiptV1,
} from './continuity-contracts.js';
export {
  AWARENESS_PEER_EVENT_MESSAGE_TYPE,
  createAwarenessEventConsumer,
} from './event-consumer.js';
export type {
  AwarenessEventConsumerOptions,
  AwarenessEventObservability,
  AwarenessEventStore,
  AwarenessPeerDelivery,
} from './event-consumer.js';
export {
  containsSecretLikeText,
  evaluateMemoryRecall,
  MEMORY_EVALUATION_CORPUS_V1,
  MEMORY_RECALL_MODES_V1,
  runMemoryEvaluationCorpus,
} from './memory-hardening.js';
export type {
  MemoryEvaluationCaseResultV1,
  MemoryEvaluationCaseV1,
  MemoryEvaluationCorpusV1,
  MemoryEvaluationQueryV1,
  MemoryEvaluationReportV1,
  MemoryEvaluationResultV1,
  MemoryRecallModeV1,
} from './memory-hardening.js';
export {
  appendWorkerLifecycleEvent,
  listWorkerLifecycleEvents,
  MAX_WORKER_LIFECYCLE_PAYLOAD_BYTES,
  MAX_WORKER_LIFECYCLE_REPLAY_LIMIT,
} from './worker-lifecycle-ledger.js';
export type {
  AppendWorkerLifecycleEventResult,
  ListWorkerLifecycleEventsOptions,
  StoredWorkerLifecycleEvent,
  WorkerLifecycleEventInput,
  WorkerLifecycleJsonValue,
  WorkerLifecycleRedaction,
} from './worker-lifecycle-ledger.js';

// Memory operations
export { insertMemory, insertMemoryWithSimilarityGate, bumpAccess } from './memory-write.js';
export { getMemory } from './memory-recall.js';
export { lexicalSearch } from './memory-search.js';
export { decayScore, findSimilarMemories } from './memory-scoring.js';
export { mineWeakness } from './memory-weakness.js';
export { forgetMemory } from './memory-lifecycle.js';
export { storeEmbedding, searchByEmbedding, loadMemoriesByIds } from './memory-embeddings.js';
export type { GuardedMemoryInsertResult } from './memory-write.js';
export type { MineWeaknessResult, MineWeaknessParams, WeaknessCluster } from './memory-weakness.js';

// Refinements
export { insertRefinement, updateRefinement, getRefinements, deleteRefinement } from './refinements.js';
export type { DeleteRefinementResult, UpdateRefinementResult } from './refinements.js';

// Intents / file locks
export { preFlightIntent } from './intents-preflight.js';
export { releaseFileLock } from './intents-release.js';
export { fileLock } from './intents-lock.js';

// Advisory file presence + optional sensitive exclusivity
export { startWork, touchWork, endWork, listWork, showWork } from './work.js';

// Collaborative plans and durable plan tasks
export { createPlan, getPlan, listPlans, joinPlan, registerPlanDocument, updatePlanStatus } from './plans.js';
export type { PlanStatus, PlanRecord, PlanDetail, PlanMemberRecord, PlanDocRecord, CreatePlanParams, JoinPlanParams, RegisterPlanDocParams } from '@octocodeai/octocode-shared/entities';
export { createTask, listTasks, listReadyTasks, addTaskDependency } from './tasks-ready.js';
export { getTask, activeTaskClaimForAgent } from './tasks-catalog.js';
export { claimTask, heartbeatTaskClaim, submitTask, releaseTaskClaim, retryTask } from './tasks-claims.js';
export type { TaskStatus, PlanTaskRecord, TaskClaimRecord, TaskRunRecord, CreateTaskParams } from '@octocodeai/octocode-shared/entities';
export type { ClaimTaskResult } from './tasks-claims.js';

// Reflection
export { reflect } from './reflect.js';

// Background operations + smart briefing + harness export
export { pruneStale } from './maintenance-stale.js';
export { notifyGet } from './maintenance-briefing.js';
export { sessionCapture, waitForLock } from './maintenance-session.js';
export { digest, inspectMaintenancePressure } from './maintenance-digest.js';
export { getWorkspaceStatus, exportMemoryDoc, exportHarness } from './maintenance-workspace.js';
export type { DigestResult, MaintenancePressure } from './maintenance-digest-types.js';
export type { BriefItem, NotifyGetResult, NotifyGetBriefResult, WaitForLockResult, PruneStaleResult } from './maintenance-stale.js';
export type { WorkspaceStatusResult } from './maintenance-workspace.js';

// Repo-readable awareness projections
export { AWARENESS_QUERY_VIEWS } from './repo-model.js';
export { queryAwareness, formatAwarenessQueryResult, renderAwarenessHtml, writeAwarenessView } from './repo-query.js';
export type { AwarenessQueryFormat, AwarenessQueryParams, AwarenessQueryResult, AwarenessQueryRow, AwarenessQuerySection, AwarenessQueryView, RepoContextInjectParams, RepoContextInjectResult, RepoContextMode } from './repo-model.js';

// Agent-native start packet
export { attendAwareness } from './attend-query.js';
export type { AttendNext } from './attend-flow.js';
export type { AttendEvidence, AttendParams, AttendResult } from './attend-model.js';

// Notifications
export { insertNotification } from './notifications-core.js';
export { getNotifications, resolveNotification } from './notifications-inbox.js';
export { pruneNotifications, agentSignal } from './notifications-signals.js';

// Verify gate
export { auditUnverified } from './verify-audit.js';
export { markVerified } from './verify-mark.js';
export type { AuditUnverifiedResult, AuditUnverifiedParams, UnverifiedIntent, StaleActiveIntent, MarkVerifiedResult, MarkVerifiedOk, MarkVerifiedErr, MarkVerifiedParams, VerifyStatus } from './verify-shared.js';

// Agent identity registry (ARCH-5)
export { registerAgent, touchAgent, resolveAgentName, resolveAgentNames, listAgents } from './agents.js';

// Pure helpers
export {
  utcNow, parseJsonList, normalizeTags, normalizeReferences,
  normalizeLabel, normalizeNotificationKind, normalizeReflectionOutcome, normalizeFilePath, tagsText, rowToMemory,
  MEMORY_LABELS, MEMORY_LABEL_VALUES, NOTIFICATION_KIND_VALUES, NOTIFICATION_KINDS,
  REFLECTION_OUTCOME_VALUES, REFLECTION_IMPORTANCE,
} from './helpers.js';

// Shared agent-tool operation runner
export { ROUTABLE_OPERATIONS, runAwarenessToolOperation } from './tool-operations.js';
export type {
  AwarenessToolOperation,
  AwarenessToolOperationContext,
  AwarenessToolOperationResult,
} from './tool-operations.js';

// Git scope
export { detectGit, fillScope, canonicalizePath, normalizeWorkspacePath } from './git.js';

// Audit log (edit_log + harness_log)
export { sha256Hex, insertEditLog, queryEditLog, insertHarnessLog, queryHarnessLog } from './audit.js';

// Doc staleness detection (edit_log-derived — no new tables)
export { mineDocStaleness, proposeDocRefresh } from './docs.js';

// Skill reference catalog (docs list|show)
export { listSkillDocs, showSkillDoc } from './docs-catalog.js';
export type { DocCatalogEntry, DocCatalogListResult, DocCatalogShowResult } from './docs-catalog.js';

// Sessions
export { insertSession, endSession, getSession, listSessions, getOrCreateSession } from './sessions.js';

// Types
export type { AgentIdentity, RegisterAgentParams, ListAgentsResult, EmbeddingSearchResult, MemoryRecord, RefinementRecord, FileLock, InsertMemoryParams, InsertMemoryResult, GetMemoryParams, GetMemoryResult, InsertRefinementParams, InsertRefinementResult, GetRefinementsParams, GetRefinementsResult, MemoryState, LockType, RunStatus, RunOrigin, WorkSource, RefinementQuality, RefinementState, ReflectionOutcome, InsertSessionParams } from './types/identity-memory.js';
export type { PreFlightRunParams, PreFlightRunResult, PreFlightRunSuccess, PreFlightRunConflict, ReleaseFileLockParams, ReleaseFileLockResult, FileLockParams, FileLockResult, FileLockReleaseStatus, ReflectParams, ReflectResult, Scope, ScopePartial } from './types/locks-reflection.js';
export type { StartWorkParams, StartWorkResult, TouchWorkParams, EndWorkParams, WorkMutationResult, ListWorkParams, ListWorkResult, WorkRunRecord, WorkFileRecord, WorkPresence, WorkPeer, WorkConflict, ForgetMemoryParams, ForgetMemoryResult, WaitForLockParams, PruneStaleParams, DeleteRefinementParams } from './types/work-maintenance.js';
export type { InsertNotificationParams, InsertNotificationResult, GetNotificationsParams, GetNotificationsResult, ResolveNotificationParams, ResolveNotificationResult, PruneNotificationsParams, PruneNotificationsResult, AgentSignalAction, AgentSignalParams, AgentSignalRecord, AgentSignalResult, NotificationRecord, NotificationKind, NotificationStatus, ExportHarnessParams, ExportHarnessResult, MemoryReferenceRow } from './types/notifications-agents.js';
export type { DocStalenessTarget, DocStalenessParams, DocStalenessEntry, DocStalenessResult, ProposeDocRefreshParams, EndSessionParams, SessionRow } from './types/plans-docs.js';

// Agent-neutral shared coordination surface. Public consumers import the package
// root; the implementation directory is not a separate product or API tier.
export { openAwarenessStore } from './coordination/open.js';
export { AwarenessStore } from './coordination/coordination-continuity.js';
export { execCli, runCli } from './coordination/cli.js';
export { dispatchAwarenessCommand } from './coordination/dispatch.js';
export { AWARENESS_COMMANDS, getCommandGroup } from './coordination/commands-spec.js';
export { defaultDbPath } from './coordination/coordination-shared.js';
export { runPreEditLockGate, checkLockConflicts, extractHookTargetPaths } from './coordination/hooks.js';
export { EXTERNAL_AGENT_AWARENESS_PROMPT, EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS, EXTERNAL_AGENT_AWARENESS_MARKER_START, EXTERNAL_AGENT_AWARENESS_MARKER_END, formatExternalAgentAwarenessInstructions, getExternalAgentAwarenessGuide, formatExternalAgentCoordinationContext } from './coordination/external-policy.js';
export { readExternalAwarenessStatus } from './coordination/external-status.js';
export { executeExternalMemoryAction, EXTERNAL_MEMORY_ACTIONS, EXTERNAL_MEMORY_RECALL_MODES, validateExternalMemoryParams } from './coordination/external-memory.js';
export { completeExternalPlanTask, finalizeExternalPlan, projectExternalPlan } from './coordination/external-plan.js';
export { detectAgentHost, generateAgentName } from './coordination/agent-naming.js';
export { storageScopeForCommand } from './workspace-policy.js';
export {
  generateOpenCodeAwarenessPlugin,
  inspectOpenCodeAwarenessPlugin,
  installOpenCodeAwarenessPlugin,
  openCodeAwarenessPluginPath,
  removeOpenCodeAwarenessPlugin,
  OPENCODE_AWARENESS_PLUGIN_FILE,
} from './opencode-plugin-adapter.js';
export type { AwarenessCommandOutcome, AwarenessCommandRequest } from './coordination/dispatch.js';
export type { CommandAction, CommandGroup, CommandParam, CommandParamType } from './coordination/commands-spec.js';
export type { HookHost, LockConflict, PreEditHookOptions, PreEditHookResult } from './coordination/hooks.js';
export type { ExternalAwarenessStatus, ExternalAwarenessTaskActivity } from './coordination/external-status.js';
export type { ExternalMemoryAction, ExternalMemoryParams, ExternalMemoryRecallMode, ExternalMemoryResult, ExternalMemoryReviewCandidate } from './coordination/external-memory.js';
export type { ObservedCheckReceipt, ExternalPlanCompletionResult, ExternalPlanProjectionInput, ExternalPlanProjectionResult, ExternalPlanProjectionStep, ExternalPlanScope } from './coordination/external-plan.js';
export type { AgentHost } from './coordination/agent-naming.js';
export type { OutboxEventV1, StoredInteractionV1 } from './coordination/coordination-continuity.js';
export type { AwarenessStorageScope } from './storage-scope.js';
export type {
  OpenCodeAwarenessInstallOptions,
  OpenCodeAwarenessInstallResult,
  OpenCodeAwarenessPluginOptions,
} from './opencode-plugin-adapter.js';

export type { RuntimeObservation } from './attend-physiology.js';

export { consolidateDatabase } from './db-consolidation.js';
export { ConsolidationContractError } from './db-consolidation-validation.js';
