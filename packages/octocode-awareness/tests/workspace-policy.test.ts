import { mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { describe, expect, it } from 'vitest';
import { selectCommand, UNKNOWN_COMMAND } from '../bin/cli-routing.js';
import {
  DEFAULT_WORKSPACE_POLICY,
  hookCommandEnabled,
  loadWorkspacePolicy,
  storageScopeForCommand,
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
      expect(storageScopeForCommand('work-command', workspace)).toBe('global');
      expect(storageScopeForCommand('attend', workspace)).toBe('global');
      expect(storageScopeForCommand('tell-memory', workspace)).toBe('global');
      expect(storageScopeForCommand('work-command', workspace, 'global')).toBe('global');
      expect(storageScopeForCommand('work-command', workspace, 'repo')).toBe('repo');
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
        policy,
      });
      expect(storageScopeForCommand('work-command', workspace)).toBe('global');
      expect(storageScopeForCommand('tell-memory', workspace)).toBe('repo');
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('maps hook profiles to the minimum required lifecycle surface', () => {
    expect(hookCommandEnabled('guard', 'pre-edit')).toBe(true);
    expect(hookCommandEnabled('guard', 'post-edit')).toBe(true);
    expect(hookCommandEnabled('guard', 'stop-verify')).toBe(true);
    expect(hookCommandEnabled('guard', 'notify-deliver')).toBe(false);
    expect(hookCommandEnabled('coordination', 'notify-deliver')).toBe(false);
    expect(hookCommandEnabled('full', 'notify-deliver')).toBe(true);
    expect(hookCommandEnabled('full', 'session-end')).toBe(true);
  });
});

describe('unified CLI facade', () => {
  it('rejects retired convenience aliases and keeps canonical noun/verb routes', () => {
    for (const argv of [['setup'], ['next'], ['inspect', 'workboard'], ['verify'], ['close'], ['init'], ['refinement', 'list']]) {
      expect(selectCommand(argv)).toEqual({ command: UNKNOWN_COMMAND, rest: argv });
    }
    expect(selectCommand(['verify', 'audit'])).toEqual({ command: 'audit-unverified', rest: [] });
    expect(selectCommand(['skill', 'install', '--platform', 'shared', '--global'])).toEqual({
      command: 'skill-install', rest: ['--platform', 'shared', '--global'],
    });
    expect(selectCommand(['work', 'end', '--run-id', 'run-1'])).toEqual({
      command: 'work-command', rest: ['--action', 'end', '--run-id', 'run-1'],
    });
  });
});
