/** Public compatibility barrel for maintenance.ts. */
export { pruneStale } from './maintenance-stale.js';
export { notifyGet, parseGitStatusShortLines } from './maintenance-briefing.js';
export { sessionCapture, waitForLock } from './maintenance-session.js';
export { inspectMaintenancePressure, digest } from './maintenance-digest.js';
export { getWorkspaceStatus, exportMemoryDoc, exportHarness } from './maintenance-workspace.js';
export type { PruneStaleResult, NotifyGetResult, SessionCaptureResult, WaitForLockResult, BriefItem, NotifyGetBriefResult } from './maintenance-stale.js';
export type { DigestResult, MaintenancePressure } from './maintenance-digest.js';
export type { WorkspaceLockEntry, WorkspaceStatusResult } from './maintenance-workspace.js';
