/**
 * @octocodeai/octocode-memory — public module API.
 *
 * Import directly — no subprocess required:
 *   import { getMemory, insertMemory, reflect } from '@octocodeai/octocode-memory';
 */

// DB layer
export { connectDb, initDb, memoryHome, resolveDbPath, hasFts, tableColumns } from './db.js';

// Memory operations
export { insertMemory, getMemory, bumpAccess, lexicalSearch, decayScore } from './memory.js';

// Refinements
export { insertRefinement, getRefinements } from './refinements.js';

// Intents / file locks
export { preFlightIntent, releaseFileLock } from './intents.js';

// Reflection
export { reflect } from './reflect.js';

// Stubs (pending full implementation)
export { pruneStale, notifyGet, sessionCapture, waitForLock } from './stubs.js';

// Verify gate
export { auditUnverified, markVerified } from './verify.js';
export type {
  AuditUnverifiedResult, AuditUnverifiedParams, UnverifiedIntent,
  MarkVerifiedResult, MarkVerifiedParams, VerifyStatus,
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
} from './types.js';
