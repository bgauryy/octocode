import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { afterEach, describe, expect, it } from 'vitest';
import { hookStateUnchanged, recordHookChangeState } from '../bin/hook-change-state.js';
import { briefingChangeSignal, overlapChangeSignal, verificationDebtSignal } from '../bin/hook-signals.js';
import { resolveDbPath } from '../src/db-runtime.js';
import { writeWorkspacePolicy } from '../src/workspace-policy.js';

const originalMemoryHome = process.env.OCTOCODE_AGENT_DIR;

afterEach(() => {
  if (originalMemoryHome === undefined) delete process.env.OCTOCODE_AGENT_DIR;
  else process.env.OCTOCODE_AGENT_DIR = originalMemoryHome;
});

describe('hook change fast path', () => {
  it('skips unchanged stores and wakes after repository state changes', () => {
    const root = mkdtempSync(join(tmpdir(), 'awareness-hook-token-'));
    const workspace = join(root, 'repo');
    mkdirSync(workspace, { recursive: true });
    writeWorkspacePolicy(workspace, { version: 1, storage: { repository: 'repo', memory: 'repo' }, hooks: { profile: 'coordination' } });
    process.env.OCTOCODE_AGENT_DIR = join(root, 'memory');
    const payload = { workspace, session_id: 'session-1' };
    try {
      expect(hookStateUnchanged(payload)).toBe(false);
      recordHookChangeState(payload);
      expect(hookStateUnchanged(payload)).toBe(true);

      const repositoryDb = resolveDbPath(null, { scope: 'repo', workspace });
      mkdirSync(dirname(repositoryDb), { recursive: true });
      writeFileSync(repositoryDb, 'changed');
      expect(hookStateUnchanged(payload)).toBe(false);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe('typed hook signals', () => {
  it('points to the changed category without leaking content', () => {
    expect(briefingChangeSignal([{ kind: 'notification' }, { kind: 'memory' }])).toBe(
      'Awareness: messages 1, memory 1.',
    );
    expect(overlapChangeSignal(2)).toBe('Awareness: overlap changed (2 paths).');
    expect(verificationDebtSignal(3)).toBe('Awareness: verification debt (3).');
  });
});
