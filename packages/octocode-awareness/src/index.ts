/**
 * @octocodeai/octocode-awareness — public module API.
 *
 * Import directly — no subprocess required:
 *   import { getMemory, insertMemory, reflect } from '@octocodeai/octocode-awareness';
 */

import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// DB layer
export { connectDb, connectCachedDb, initDb, memoryHome, resolveDbPath, hasFts, tableColumns, replaceMemoryReferences, referenceKind, evictExpiredLocks } from './db.js';

// Memory operations
export { insertMemory, getMemory, bumpAccess, lexicalSearch, decayScore, findSimilarMemories, mineWeakness, forgetMemory, storeEmbedding, searchByEmbedding, loadMemoriesByIds } from './memory.js';
export type { MineWeaknessResult, MineWeaknessParams, WeaknessCluster } from './memory.js';
export { resolveEmbedCommand, runHostEmbedder } from './embed-host.js';
export type { HostEmbedding } from './embed-host.js';

// Refinements
export { insertRefinement, getRefinements, deleteRefinement } from './refinements.js';
export type { DeleteRefinementResult } from './refinements.js';

// Intents / file locks
export { preFlightIntent, releaseFileLock, fileLock } from './intents.js';

// Reflection
export { reflect } from './reflect.js';

// Background operations + smart briefing + harness export
export { pruneStale, notifyGet, sessionCapture, waitForLock, digest, getWorkspaceStatus, exportMemoryDoc, exportHarness } from './maintenance.js';
export type { DigestResult, BriefItem, NotifyGetResult, NotifyGetBriefResult, WorkspaceStatusResult, WorkspaceLockEntry, WaitForLockResult, PruneStaleResult } from './maintenance.js';

// Repo-readable awareness projections
export {
  AWARENESS_QUERY_VIEWS,
  queryAwareness,
  formatAwarenessQueryResult,
  renderAwarenessHtml,
  writeAwarenessView,
  injectRepoContext,
} from './repo-context.js';
export type {
  AwarenessQueryFormat,
  AwarenessQueryParams,
  AwarenessQueryResult,
  AwarenessQueryRow,
  AwarenessQuerySection,
  AwarenessQueryView,
  RepoContextInjectParams,
  RepoContextInjectResult,
  RepoContextMode,
} from './repo-context.js';

// Agent-native start packet
export { attendAwareness } from './attend.js';
export type { AttendEvidence, AttendParams, AttendResult } from './attend.js';

// Notifications
export { insertNotification, getNotifications, resolveNotification, pruneNotifications, agentSignal } from './notifications.js';

// Pi native hook adapter
export {
  createPiAwarenessBridge,
  extractPiWriteTargetPaths,
  getPiAwarenessAgentId,
  getPiAwarenessSessionId,
  wirePiAwarenessHooks,
} from './pi-hooks.js';
export type {
  PiAwarenessBridgeOptions,
  PiLikeApi,
  PiLikeContext,
  PiLikeSessionManager,
  PiLikeUi,
  PiToolEvent,
} from './pi-hooks.js';

// Verify gate
export { auditUnverified, markVerified } from './verify.js';
export type {
  AuditUnverifiedResult, AuditUnverifiedParams, UnverifiedIntent, StaleActiveIntent,
  MarkVerifiedResult, MarkVerifiedOk, MarkVerifiedErr, MarkVerifiedParams, VerifyStatus,
} from './verify.js';

// Agent identity registry (ARCH-5)
export { registerAgent, touchAgent, resolveAgentName, resolveAgentNames, listAgents } from './agents.js';

// Pure helpers
export {
  utcNow, parseJsonList, normalizeTags, normalizeReferences,
  normalizeLabel, normalizeFilePath, tagsText, rowToMemory,
  MEMORY_LABELS, MEMORY_LABEL_VALUES, REFLECTION_IMPORTANCE,
} from './helpers.js';

// Shared agent-tool operation runner
export { runAwarenessToolOperation } from './tool-operations.js';
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
export type {
  AgentIdentity, RegisterAgentParams, ListAgentsResult, EmbeddingSearchResult,
  MemoryRecord, RefinementRecord, FileLock,
  InsertMemoryParams, InsertMemoryResult,
  GetMemoryParams, GetMemoryResult,
  InsertRefinementParams, InsertRefinementResult,
  GetRefinementsParams, GetRefinementsResult,
  PreFlightTaskParams, PreFlightTaskResult, PreFlightTaskSuccess, PreFlightTaskConflict,
  ReleaseFileLockParams, ReleaseFileLockResult, TaskParams, FileLockParams, FileLockResult, FileLockStatusEntry,
  ReflectParams, ReflectResult,
  Scope, ScopePartial,
  MemoryState, LockType, TaskStatus, RefinementQuality, RefinementState, ReflectionOutcome,
  // New types
  ForgetMemoryParams, ForgetMemoryResult,
  WaitForLockParams,
  PruneStaleParams,
  DeleteRefinementParams,
  InsertNotificationParams, InsertNotificationResult,
  GetNotificationsParams, GetNotificationsResult,
  ResolveNotificationParams, ResolveNotificationResult,
  PruneNotificationsParams, PruneNotificationsResult,
  AgentSignalAction, AgentSignalParams, AgentSignalRecord, AgentSignalResult,
  NotificationRecord, NotificationKind, NotificationStatus,
  ExportHarnessParams, ExportHarnessResult,
  MemoryReferenceRow,
  DocStalenessTarget, DocStalenessParams, DocStalenessEntry, DocStalenessResult,
  ProposeDocRefreshParams,
} from './types.js';

function runCliWhenExecutedDirectly(): void {
  const invokedPath = process.argv[1] ? resolve(process.argv[1]) : null;
  const modulePath = fileURLToPath(import.meta.url);
  if (invokedPath !== modulePath) return;

  const cliPath = resolve(dirname(modulePath), 'bin/awareness.js');
  const result = spawnSync(process.execPath, [cliPath, ...process.argv.slice(2)], {
    stdio: 'inherit',
  });
  process.exit(result.status ?? 1);
}

runCliWhenExecutedDirectly();
