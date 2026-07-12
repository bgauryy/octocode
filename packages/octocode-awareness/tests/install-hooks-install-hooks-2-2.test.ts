import { describe, expect, it } from 'vitest';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runHooksInstall } from '../src/hooks-install.js';
const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(TEST_DIR, '../../..');
const SCRIPT = resolve(REPO_ROOT, 'packages/octocode-awareness/out/octocode-awareness.js');
const NODE = process.execPath;
function runInstallHooks(args: string[], script = SCRIPT) {
    const result = spawnSync(NODE, [script, ...args], {
        encoding: 'utf8',
        timeout: 5000,
    });
    expect(result.status).toBe(0);
    return JSON.parse(result.stdout) as {
        host: string;
        settingsPath: string;
        resultingSettings: {
            version?: number;
            hooks?: Record<string, Array<Record<string, unknown>>>;
        };
    };
}
function runInstallHooksRaw(args: string[], script = SCRIPT) {
    return spawnSync(NODE, [script, ...args], {
        encoding: 'utf8',
        timeout: 5000,
    });
}

describe('install-hooks', () => {
it('previews Cursor hooks in native .cursor/hooks.json shape', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-cursor-hooks-'));
    try {
      const result = runInstallHooks(['hooks', 'install', '--host', 'cursor', '--project-dir', projectDir, '--dry-run']);

      expect(result.host).toBe('cursor');
      expect(result.settingsPath).toBe(resolve(projectDir, '.cursor/hooks.json'));
      expect(result.resultingSettings).toMatchObject({ version: 1 });
      expect(Object.keys(result.resultingSettings.hooks ?? {})).toEqual([
        'preToolUse',
        'postToolUse',
                  'postToolUseFailure',
                  'subagentStart',
                  'stop',
        'subagentStop',
        'sessionEnd',
        'preCompact',
        'sessionStart',
      ]);
      expect(result.resultingSettings.hooks?.preToolUse?.[0]).toMatchObject({
        command: expect.stringContaining('hook-runner.mjs'),
        timeout: 20,
        matcher: expect.stringContaining('Write'),
      });
      expect(result.resultingSettings.hooks?.preToolUse?.[0]?.command).toContain(
        ' pre-edit --host cursor --skill-root ',
      );
      expect(result.resultingSettings.hooks?.preToolUse?.[0]).not.toHaveProperty('hooks');
      expect(result.resultingSettings.hooks?.preToolUse).toHaveLength(1);
      expect(JSON.stringify(result.resultingSettings.hooks?.preCompact)).toContain(
        ' session-compact --host cursor --skill-root ',
      );
      expect(JSON.stringify(result.resultingSettings.hooks?.sessionEnd)).toContain(
        ' session-end --host cursor --skill-root ',
      );
      expect(JSON.stringify(result.resultingSettings.hooks?.preToolUse)).not.toContain(
        ' harness-guard --host cursor ',
      );
      expect(JSON.stringify(result.resultingSettings)).not.toContain('CLAUDE_PROJECT_DIR');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
it('strict check rejects an exact config whose hook target disappeared', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-missing-hook-target-'));
    const hookDir = resolve(projectDir, 'portable-skill/scripts/hooks');
    try {
      mkdirSync(hookDir, { recursive: true });
      for (const name of [
        'pre-edit.sh', 'post-edit.sh', 'stop-verify.sh', 'session-compact.sh',
        'session-end.sh', 'notify-deliver.sh',
      ]) {
        writeFileSync(resolve(hookDir, name), '#!/bin/sh\n');
      }
      const runner = resolve(hookDir, '..', 'hook-runner.mjs');
      writeFileSync(runner, '#!/usr/bin/env node\n');
      const installed = runHooksInstall(['--host', 'codex', '--project-dir', projectDir], {
        cwd: projectDir,
        hookDir,
      });
      expect(installed.exitCode).toBe(0);
      rmSync(runner);

      const checked = runHooksInstall([
        '--host', 'codex', '--project-dir', projectDir, '--check', '--strict',
      ], { cwd: projectDir, hookDir });
      expect(checked.exitCode).toBe(2);
      expect(checked.payload).toMatchObject({
        ok: false,
        health: { config: { status: 'needs_repair' } },
      });
      expect(JSON.stringify(checked.payload)).toContain('target_missing');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
it('removes Cursor awareness hooks without deleting unrelated flat hooks', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-cursor-remove-'));
    const unrelated = '/tmp/unrelated-cursor-hook.sh';
    const preEdit = resolve(
      REPO_ROOT,
      'skills/octocode-awareness/scripts/hooks/pre-edit.sh',
    );
    try {
      mkdirSync(resolve(projectDir, '.cursor'), { recursive: true });
      writeFileSync(
        resolve(projectDir, '.cursor/hooks.json'),
        JSON.stringify({
          version: 1,
          hooks: {
            preToolUse: [
              { command: unrelated, timeout: 20, matcher: 'Write' },
              { command: preEdit, timeout: 20, matcher: 'Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch' },
            ],
          },
        }, null, 2),
      );

      const result = runInstallHooks(['hooks', 'remove', '--host', 'cursor', '--project-dir', projectDir, '--dry-run']);

      expect(result.resultingSettings.hooks?.preToolUse).toEqual([
        { command: unrelated, timeout: 20, matcher: 'Write' },
      ]);
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
it('strict check reports drifted hooks and install repairs them', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-codex-drift-'));
    const preEdit = resolve(
      REPO_ROOT,
      'skills/octocode-awareness/scripts/hooks/pre-edit.sh',
    );
    try {
      mkdirSync(resolve(projectDir, '.codex'), { recursive: true });
      writeFileSync(
        resolve(projectDir, '.codex/hooks.json'),
        JSON.stringify({
          hooks: {
            PreToolUse: [
              {
                matcher: 'Write',
                hooks: [{ type: 'command', command: preEdit, timeout: 5 }],
              },
            ],
          },
        }, null, 2),
      );

      const check = runInstallHooksRaw(['hooks', 'check', '--host', 'codex', '--project-dir', projectDir, '--strict', '--compact']);
      expect(check.status).toBe(2);
      const parsed = JSON.parse(check.stdout) as {
        ok: boolean;
        health: { config: string; runtime: string };
        drifted: string[];
      };
      expect(parsed.ok).toBe(false);
      expect(parsed.health).toMatchObject({ config: 'needs_repair', runtime: 'unverified', coverage: '0/8' });
      expect(parsed.drifted).toContain('PreToolUse:pre-edit.sh');

      const repaired = runInstallHooks(['hooks', 'install', '--host', 'codex', '--project-dir', projectDir, '--dry-run']);
      const preToolUse = repaired.resultingSettings.hooks?.PreToolUse ?? [];
      const preEditEntries = preToolUse.filter((entry) => {
        const serialized = JSON.stringify(entry);
        return serialized.includes('hook-runner.mjs')
          && serialized.includes(' pre-edit --host codex --skill-root ');
      });
      expect(preEditEntries).toHaveLength(1);
      expect(preEditEntries[0]).toMatchObject({
        matcher: '^(?:apply_patch|Write|Edit)$',
      });
      expect(JSON.stringify(preEditEntries[0])).toContain('"timeout":20');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
it('removes the obsolete standalone harness guard during repair', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-obsolete-guard-'));
    const hookDir = resolve(REPO_ROOT, 'skills/octocode-awareness/scripts/hooks');
    try {
      mkdirSync(resolve(projectDir, '.codex'), { recursive: true });
      writeFileSync(resolve(projectDir, '.codex/hooks.json'), JSON.stringify({
        hooks: {
          PreToolUse: [{
            matcher: 'Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch',
            hooks: [{ type: 'command', command: resolve(hookDir, 'harness-guard.sh'), timeout: 20 }],
          }],
        },
      }));

      const repaired = runHooksInstall(['--host', 'codex', '--project-dir', projectDir, '--dry-run'], {
        cwd: projectDir,
        hookDir,
      });
      expect(repaired.exitCode).toBe(0);
      expect(JSON.stringify(repaired.payload)).not.toContain('harness-guard.sh');
      expect(JSON.stringify(repaired.payload)).toContain('hook-runner.mjs');
      expect(JSON.stringify(repaired.payload)).toContain(' pre-edit --host codex --skill-root ');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
it('strict check reports a stale awareness hook root as drifted', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-codex-stale-root-'));
    const stalePreEdit = resolve(
      REPO_ROOT,
      'skills/octocode-awareness/scripts/hooks/pre-edit.sh',
    );
    try {
      mkdirSync(resolve(projectDir, '.codex'), { recursive: true });
      writeFileSync(
        resolve(projectDir, '.codex/hooks.json'),
        JSON.stringify({
          hooks: {
            PreToolUse: [
              {
                matcher: 'Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch',
                hooks: [{ type: 'command', command: stalePreEdit, timeout: 20 }],
              },
            ],
          },
        }, null, 2),
      );

      const check = runInstallHooksRaw(['hooks', 'check', '--host', 'codex', '--project-dir', projectDir, '--strict', '--compact']);
      expect(check.status).toBe(2);
      const parsed = JSON.parse(check.stdout) as {
        ok: boolean;
        health: { config: string; runtime: string };
        drifted: string[];
      };
      expect(parsed.ok).toBe(false);
      expect(parsed.health).toMatchObject({ config: 'needs_repair', runtime: 'unverified', coverage: '0/8' });
      expect(parsed.drifted).toContain('PreToolUse:pre-edit.sh');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
it('strict check reports exact hooks with stale duplicate awareness entries', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-codex-duplicate-drift-'));
    const preEdit = resolve(
      REPO_ROOT,
      'skills/octocode-awareness/scripts/hooks/pre-edit.sh',
    );
    try {
      const exact = runInstallHooks(['hooks', 'install', '--host', 'codex', '--project-dir', projectDir, '--dry-run']);
      mkdirSync(resolve(projectDir, '.codex'), { recursive: true });
      const settings = exact.resultingSettings;
      settings.hooks?.PreToolUse?.push({
        matcher: 'Write',
        hooks: [{ type: 'command', command: preEdit, timeout: 5 }],
      });
      writeFileSync(resolve(projectDir, '.codex/hooks.json'), JSON.stringify(settings, null, 2));

      const check = runInstallHooksRaw(['hooks', 'check', '--host', 'codex', '--project-dir', projectDir, '--strict']);
      expect(check.status).toBe(2);
      const parsed = JSON.parse(check.stdout) as {
        ok: boolean;
        installed: {
          installed_all: boolean;
          drifted: string[];
          details: Record<string, { matching_count: number; drifted: boolean }>;
        };
      };
      expect(parsed.ok).toBe(false);
      expect(parsed.installed.installed_all).toBe(true);
      expect(parsed.installed.drifted).toContain('PreToolUse:pre-edit.sh');
      expect(parsed.installed.details['PreToolUse:pre-edit.sh']?.matching_count).toBe(2);

      const repaired = runInstallHooks(['hooks', 'install', '--host', 'codex', '--project-dir', projectDir, '--dry-run']);
      const preToolUse = repaired.resultingSettings.hooks?.PreToolUse ?? [];
      const preEditEntries = preToolUse.filter((entry) => {
        const serialized = JSON.stringify(entry);
        return serialized.includes('hook-runner.mjs')
          && serialized.includes(' pre-edit --host codex --skill-root ');
      });
      expect(preEditEntries).toHaveLength(1);
      expect(preEditEntries[0]).toMatchObject({
        matcher: '^(?:apply_patch|Write|Edit)$',
      });
      expect(JSON.stringify(preEditEntries[0])).toContain('"timeout":20');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

});
