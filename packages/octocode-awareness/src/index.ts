/**
 * @octocodeai/octocode-awareness — public module API.
 *
 * Import directly — no subprocess required:
 *   import { getMemory, insertMemory, reflect } from '@octocodeai/octocode-awareness';
 */

// DB layer
export { connectDb, initDb, memoryHome, resolveDbPath, hasFts, tableColumns, replaceMemoryReferences, referenceKind, ensureMemoryReferencesVersion } from './db.js';

// Memory operations
export { insertMemory, getMemory, bumpAccess, lexicalSearch, decayScore, findSimilarMemories, mineWeakness, forgetMemory } from './memory.js';
export type { MineWeaknessResult, MineWeaknessParams, WeaknessCluster } from './memory.js';

// Refinements
export { insertRefinement, getRefinements, deleteRefinement } from './refinements.js';
export type { DeleteRefinementResult } from './refinements.js';

// Intents / file locks
export { preFlightIntent, releaseFileLock } from './intents.js';

// Reflection
export { reflect } from './reflect.js';

// Background operations + smart briefing + harness export
export { pruneStale, notifyGet, sessionCapture, waitForLock, digest, getWorkspaceStatus, exportMemoryDoc, exportHarness } from './stubs.js';
export type { DigestResult, BriefItem, NotifyGetBriefResult, WorkspaceStatusResult, WorkspaceLockEntry, WaitForLockResult, PruneStaleResult } from './stubs.js';

// Notifications
export { insertNotification, getNotifications, resolveNotification, pruneNotifications } from './notifications.js';

// Pi native hook adapter
export {
  createPiAwarenessBridge,
  extractPiWriteTargetPaths,
  getPiAwarenessAgentId,
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
  AuditUnverifiedResult, AuditUnverifiedParams, UnverifiedIntent,
  MarkVerifiedResult, MarkVerifiedOk, MarkVerifiedErr, MarkVerifiedParams, VerifyStatus,
} from './verify.js';

// Pure helpers
export {
  utcNow, parseJsonList, normalizeTags, normalizeReferences,
  normalizeLabel, normalizeFilePath, tagsText, rowToMemory,
  MEMORY_LABELS, REFLECTION_IMPORTANCE,
} from './helpers.js';

// Git scope
export { detectGit, fillScope } from './git.js';

// Types
export type {
  MemoryRecord, RefinementRecord, IntentRecord, FileLock,
  InsertMemoryParams, InsertMemoryResult,
  GetMemoryParams, GetMemoryResult,
  InsertRefinementParams, InsertRefinementResult,
  GetRefinementsParams, GetRefinementsResult,
  PreFlightIntentParams, PreFlightIntentResult, PreFlightIntentSuccess, PreFlightIntentConflict,
  ReleaseFileLockParams, ReleaseFileLockResult,
  ReflectParams, ReflectResult,
  Scope, ScopePartial,
  MemoryState, LockType, IntentStatus, RefinementQuality, RefinementState, ReflectionOutcome,
  // New types
  ForgetMemoryParams, ForgetMemoryResult,
  WaitForLockParams,
  PruneStaleParams,
  DeleteRefinementParams,
  InsertNotificationParams, InsertNotificationResult,
  GetNotificationsParams, GetNotificationsResult,
  ResolveNotificationParams, ResolveNotificationResult,
  PruneNotificationsParams, PruneNotificationsResult,
  NotificationRecord, NotificationKind, NotificationStatus,
  ExportHarnessParams, ExportHarnessResult,
  MemoryReferenceRow,
} from './types.js';
