import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { initDb } from '../src/db.js';
import { hookReceipts, hookRuntimeReceiptHealth, upsertHookReceipt } from '../src/hook-receipts.js';
import { runHooksInstall } from '../src/hooks-install.js';

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
  });

  it('treats complete Claude skill frontmatter as a definition, not settings config', () => {
    const projectDir = mkdtempSync(join(tmpdir(), 'octocode-frontmatter-health-'));
    const skillDir = join(projectDir, '.claude', 'skills', 'octocode-awareness');
    const hookDir = join(skillDir, 'scripts', 'hooks');
    const dbPath = join(projectDir, 'awareness.sqlite3');
    mkdirSync(hookDir, { recursive: true });
    mkdirSync(skillDir, { recursive: true });
    writeFileSync(join(hookDir, '..', 'hook-runner.mjs'), '#!/usr/bin/env node\n');
    for (const hook of ['pre-edit', 'post-edit', 'notify-deliver', 'stop-verify', 'session-compact', 'session-end']) {
      writeFileSync(join(hookDir, `${hook}.sh`), '#!/bin/sh\n');
    }
    writeFileSync(join(skillDir, 'SKILL.md'), `---
name: octocode-awareness
hooks:
  PreToolUse: pre-edit
  PostToolUse: post-edit
  PostToolUseFailure: post-edit
  SubagentStart: notify-deliver
  Stop: stop-verify
  SubagentStop: stop-verify
  PreCompact: session-compact
  SessionEnd: session-end
  UserPromptSubmit: notify-deliver
---
# Awareness
`);
    const database = new DatabaseSync(dbPath);
    initDb(database);
    upsertHookReceipt(database, {
      workspacePath: projectDir, host: 'claude', event: 'PreToolUse', status: 'success',
    });
    database.close();

    try {
      const checked = runHooksInstall([
        '--host', 'claude', '--project-dir', projectDir, '--check', '--strict', '--compact',
      ], { cwd: projectDir, hookDir, dbPath });
      expect(checked.exitCode).toBe(0);
      expect(checked.payload).toMatchObject({
        ok: true,
        surface: 'skill_frontmatter',
        health: {
          definition: 'ready', config: 'not_required', activation: 'unverified',
          runtime: 'observed', coverage: '1/9',
        },
      });
      expect(Buffer.byteLength(JSON.stringify(checked.payload), 'utf8')).toBeLessThanOrEqual(256);

      const installed = runHooksInstall(['--host', 'claude', '--project-dir', projectDir], {
        cwd: projectDir, hookDir, dbPath,
      });
      expect(installed.exitCode).toBe(0);
      const settingsCheck = runHooksInstall([
        '--host', 'claude', '--project-dir', projectDir, '--check', '--strict', '--compact',
      ], { cwd: projectDir, hookDir, dbPath });
      expect(settingsCheck.payload).toMatchObject({ surface: 'settings', health: { config: 'ready' } });
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
});
