/**
 * stubs.ts — backward-compatibility re-export shim.
 *
 * All implementations have moved to ./maintenance.ts.
 * This file exists only so any dist/.d.ts consumers that reference
 * the old 'stubs.js' path continue to resolve correctly.
 *
 * Do not add new code here. Edit maintenance.ts instead.
 */
export {
  pruneStale,
  notifyGet,
  sessionCapture,
  waitForLock,
  digest,
  getWorkspaceStatus,
  exportMemoryDoc,
  exportHarness,
} from './maintenance.js';
export type {
  DigestResult,
  BriefItem,
  NotifyGetResult,
  NotifyGetBriefResult,
  WorkspaceStatusResult,
  WorkspaceLockEntry,
  WaitForLockResult,
  PruneStaleResult,
} from './maintenance.js';
