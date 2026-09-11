import type { DatabaseSync } from 'node:sqlite';
import { connectDb, resolveDbPath } from './db-runtime.js';
import { canonicalizePath } from './git.js';
import { utcNow } from './helpers.js';
import type { HookHost } from './hooks-install-specs.js';
import { storageScopeForCommand } from './workspace-policy.js';

export type HookReceiptStatus = 'success' | 'failure';
export type HookReceiptHost = HookHost | 'opencode';

export interface HookReceipt {
  workspace_path: string;
  host: HookReceiptHost;
  event: string;
  status: HookReceiptStatus;
  last_seen_at: string;
}

export const HOOK_RECEIPT_STALE_MS = 7 * 24 * 60 * 60_000;
export const HOOK_RECEIPT_SUCCESS_SAMPLE_MS = 5 * 60_000;

export function upsertHookReceipt(db: DatabaseSync, receipt: {
  workspacePath: string;
  host: HookReceiptHost;
  event: string;
  status: HookReceiptStatus;
  observedAt?: string;
  minimumIntervalMs?: number;
}): boolean {
  const workspacePath = canonicalizePath(receipt.workspacePath);
  const event = receipt.event.trim().slice(0, 128);
  if (!event) return false;
  const observedAt = receipt.observedAt ?? utcNow();
  const minimumIntervalMs = receipt.minimumIntervalMs ?? 0;
  if (receipt.status === 'success' && minimumIntervalMs > 0) {
    const previous = db.prepare(`SELECT status, last_seen_at FROM hook_receipts
      WHERE workspace_path = ? AND host = ? AND event = ?`)
      .get(workspacePath, receipt.host, event) as { status: HookReceiptStatus; last_seen_at: string } | undefined;
    const previousMs = previous ? Date.parse(previous.last_seen_at) : Number.NaN;
    const observedMs = Date.parse(observedAt);
    if (previous?.status === 'success'
      && Number.isFinite(previousMs)
      && Number.isFinite(observedMs)
      && observedMs >= previousMs
      && observedMs - previousMs < minimumIntervalMs) {
      return false;
    }
  }
  db.prepare(`INSERT INTO hook_receipts(workspace_path, host, event, status, last_seen_at)
    VALUES (?, ?, ?, ?, ?)
    ON CONFLICT(workspace_path, host, event) DO UPDATE SET
      status = excluded.status,
      last_seen_at = excluded.last_seen_at`)
    .run(workspacePath, receipt.host, event, receipt.status, observedAt);
  return true;
}

export function recordHookReceiptBestEffort(receipt: {
  workspacePath: string;
  host: HookReceiptHost;
  event: string;
  status: HookReceiptStatus;
  observedAt?: string;
  minimumIntervalMs?: number;
}): boolean {
  let database: DatabaseSync | undefined;
  try {
    database = connectDb(resolveDbPath(null, {
      workspace: receipt.workspacePath,
      scope: storageScopeForCommand('hook', receipt.workspacePath),
    }));
    return upsertHookReceipt(database, receipt);
  } catch {
    // Hook execution remains authoritative; telemetry must never block it.
    return false;
  } finally {
    try { database?.close(); } catch { /* best effort */ }
  }
}

export function hookReceipts(
  db: DatabaseSync,
  workspacePath: string,
  host: HookReceiptHost,
): HookReceipt[] {
  return db.prepare(`SELECT workspace_path, host, event, status, last_seen_at
    FROM hook_receipts WHERE workspace_path = ? AND host = ? ORDER BY event`)
    .all(canonicalizePath(workspacePath), host) as unknown as HookReceipt[];
}

export function hookRuntimeReceiptHealth(
  receipts: HookReceipt[],
  expectedEvents: string[],
  nowMs = Date.now(),
): { status: 'observed' | 'unverified' | 'stale' | 'failed'; last_seen: string | null; coverage: string } {
  const expected = new Set(expectedEvents);
  const relevant = receipts.filter((receipt) => expected.has(receipt.event));
  const lastSeen = relevant.reduce<string | null>((latest, receipt) => (
    latest == null || receipt.last_seen_at > latest ? receipt.last_seen_at : latest
  ), null);
  const coverage = `${relevant.length}/${expected.size}`;
  if (relevant.length === 0) return { status: 'unverified', last_seen: null, coverage };
  if (relevant.some((receipt) => receipt.status === 'failure')) return { status: 'failed', last_seen: lastSeen, coverage };
  const lastSeenMs = lastSeen == null ? Number.NaN : new Date(lastSeen).getTime();
  if (!Number.isFinite(lastSeenMs) || nowMs - lastSeenMs > HOOK_RECEIPT_STALE_MS) {
    return { status: 'stale', last_seen: lastSeen, coverage };
  }
  return { status: 'observed', last_seen: lastSeen, coverage };
}
