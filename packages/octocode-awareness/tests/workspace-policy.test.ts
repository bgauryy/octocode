import { mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { describe, expect, it } from 'vitest';
import {
  DEFAULT_WORKSPACE_POLICY,
  claimNativeHookOwner,
  hookCommandEnabled,
  hookIntegrationOwner,
  loadWorkspacePolicy,
  storageScopeForOperation,
  workspacePolicyPath,
  writeWorkspacePolicy,
} from '../src/workspace-policy.js';

describe('workspace Awareness policy', () => {
  it('defaults coordination and memory to one home store and hooks to coordination', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-policy-'));
    try {
      expect(loadWorkspacePolicy(workspace)).toEqual({
        path: workspacePolicyPath(workspace),
        exists: false,
        policy: DEFAULT_WORKSPACE_POLICY,
      });
      expect(storageScopeForOperation('work.create', workspace)).toBe('global');
      expect(storageScopeForOperation('context.orient', workspace)).toBe('global');
      expect(storageScopeForOperation('memory.record', workspace)).toBe('global');
      expect(storageScopeForOperation('work.create', workspace, 'global')).toBe('global');
      expect(storageScopeForOperation('work.create', workspace, 'repo')).toBe('repo');
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('persists and reloads explicit repository policy', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-policy-'));
    try {
      const policy = {
        version: 1 as const,
        storage: { repository: 'global' as const, memory: 'repo' as const },
        hooks: { profile: 'full' as const },
      };
      expect(writeWorkspacePolicy(workspace, policy)).toBe(workspacePolicyPath(workspace));
      expect(loadWorkspacePolicy(workspace)).toEqual({
        path: workspacePolicyPath(workspace),
        exists: true,
        policy: { ...policy, hooks: { ...policy.hooks, owners: DEFAULT_WORKSPACE_POLICY.hooks.owners } },
      });
      expect(storageScopeForOperation('work.create', workspace)).toBe('global');
      expect(storageScopeForOperation('memory.record', workspace)).toBe('repo');
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('persists one native integration owner without changing storage or profile', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-policy-owner-'));
    try {
      writeWorkspacePolicy(workspace, {
        version: 1,
        storage: { repository: 'repo', memory: 'global' },
        hooks: { profile: 'full' },
      });
      expect(hookIntegrationOwner(workspace, 'codex')).toBe('shell');
      expect(claimNativeHookOwner({ workspace, host: 'codex' })).toMatchObject({ changed: true, owner: 'native' });
      expect(claimNativeHookOwner({ workspace, host: 'codex' })).toMatchObject({ changed: false, owner: 'native' });
      expect(loadWorkspacePolicy(workspace).policy).toMatchObject({
        storage: { repository: 'repo', memory: 'global' },
        hooks: { profile: 'full', owners: { codex: 'native', pi: 'native' } },
      });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('maps hook profiles to the minimum required lifecycle surface', () => {
    expect(hookCommandEnabled('guard', 'pre-edit')).toBe(true);
    expect(hookCommandEnabled('guard', 'post-edit')).toBe(true);
    expect(hookCommandEnabled('guard', 'stop-verify')).toBe(true);
    expect(hookCommandEnabled('guard', 'notify-deliver')).toBe(false);
    expect(hookCommandEnabled('coordination', 'notify-deliver')).toBe(true);
    expect(hookCommandEnabled('coordination', 'post-edit')).toBe(false);
    expect(hookCommandEnabled('coordination', 'session-end')).toBe(true);
    expect(hookCommandEnabled('coordination', 'pre-edit')).toBe(false);
    expect(hookCommandEnabled('coordination', 'stop-verify')).toBe(false);
    expect(hookCommandEnabled('coordination', 'session-compact')).toBe(false);
    expect(hookCommandEnabled('full', 'notify-deliver')).toBe(true);
    expect(hookCommandEnabled('full', 'session-end')).toBe(true);
  });
});
