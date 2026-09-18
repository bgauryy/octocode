import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db-init.js';
import { hookReceipts, hookRuntimeReceiptHealth, upsertHookReceipt } from '../src/hook-receipts.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  initDb(db);
  return db;
}

describe('hook runtime receipts', () => {
  it('keeps only the last bounded observation per workspace, host, and event', () => {
    const db = freshDb();
    upsertHookReceipt(db, {
      workspacePath: '/tmp/receipt-workspace', host: 'codex', event: 'PreToolUse',
      status: 'success', observedAt: '2026-07-12T08:00:00Z',
    });
    upsertHookReceipt(db, {
      workspacePath: '/tmp/receipt-workspace', host: 'codex', event: 'PreToolUse',
      status: 'failure', observedAt: '2026-07-12T09:00:00Z',
    });

    const receipts = hookReceipts(db, '/tmp/receipt-workspace', 'codex');
    expect(receipts).toHaveLength(1);
    expect(receipts[0]).toMatchObject({ event: 'PreToolUse', status: 'failure', last_seen_at: '2026-07-12T09:00:00Z' });
    expect(hookRuntimeReceiptHealth(receipts, ['PreToolUse', 'PostToolUse']))
      .toMatchObject({ status: 'failed', coverage: '1/2' });
  });

  it('samples repeated successful observations but records recovery after failure immediately', () => {
    const db = freshDb();
    const receipt = {
      workspacePath: '/tmp/sampled-receipt-workspace', host: 'codex' as const, event: 'UserPromptSubmit',
      status: 'success' as const, minimumIntervalMs: 5 * 60_000,
    };
    upsertHookReceipt(db, { ...receipt, observedAt: '2026-07-12T08:00:00Z' });
    upsertHookReceipt(db, { ...receipt, observedAt: '2026-07-12T08:01:00Z' });
    expect(hookReceipts(db, receipt.workspacePath, receipt.host)[0]?.last_seen_at)
      .toBe('2026-07-12T08:00:00Z');

    upsertHookReceipt(db, { ...receipt, status: 'failure', observedAt: '2026-07-12T08:02:00Z' });
    expect(hookReceipts(db, receipt.workspacePath, receipt.host)[0]).toMatchObject({
      status: 'failure', last_seen_at: '2026-07-12T08:02:00Z',
    });
    upsertHookReceipt(db, { ...receipt, observedAt: '2026-07-12T08:03:00Z' });
    expect(hookReceipts(db, receipt.workspacePath, receipt.host)[0]).toMatchObject({
      status: 'success', last_seen_at: '2026-07-12T08:03:00Z',
    });
    db.close();
  });

  it('distinguishes observed, stale, and unverified runtime evidence', () => {
    const now = Date.parse('2026-07-12T12:00:00Z');
    expect(hookRuntimeReceiptHealth([], ['PreToolUse'], now)).toEqual({
      status: 'unverified', last_seen: null, coverage: '0/1',
    });
    expect(hookRuntimeReceiptHealth([{
      workspace_path: '/tmp/ws', host: 'claude', event: 'PreToolUse', status: 'success',
      last_seen_at: '2026-07-12T11:59:00Z',
    }], ['PreToolUse'], now)).toMatchObject({ status: 'observed', coverage: '1/1' });
    expect(hookRuntimeReceiptHealth([{
      workspace_path: '/tmp/ws', host: 'claude', event: 'PreToolUse', status: 'success',
      last_seen_at: '2026-06-01T00:00:00Z',
    }], ['PreToolUse'], now)).toMatchObject({ status: 'stale', coverage: '1/1' });
    expect(hookRuntimeReceiptHealth([{
      workspace_path: '/tmp/ws', host: 'claude', event: 'PreToolUse', status: 'degraded',
      last_seen_at: '2026-07-12T11:59:00Z',
    }], ['PreToolUse'], now)).toMatchObject({ status: 'degraded', coverage: '1/1' });
  });

});
