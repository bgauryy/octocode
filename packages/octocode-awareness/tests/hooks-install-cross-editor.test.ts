import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import { runHooksInstall } from '../src/hooks-install-command.js';

function fixture() {
  const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-cross-editor-hooks-'));
  const hookDir = resolve(projectDir, 'skills/octocode-awareness/scripts/hooks');
  mkdirSync(hookDir, { recursive: true });
  writeFileSync(resolve(hookDir, '..', 'hook-runner.mjs'), '#!/usr/bin/env node\n');
  return { projectDir, hookDir };
}

describe('cross-editor hook installation', () => {
  it('installs official GitHub Copilot v1 hooks without disturbing sibling configuration', () => {
    const { projectDir, hookDir } = fixture();
    const settingsPath = resolve(projectDir, '.github/hooks/octocode-awareness.json');
    try {
      mkdirSync(resolve(projectDir, '.github/hooks'), { recursive: true });
      writeFileSync(settingsPath, JSON.stringify({
        version: 1,
        disableAllHooks: false,
        ownerMetadata: { team: 'platform' },
        hooks: {
          preToolUse: [{ type: 'command', command: './scripts/unrelated.sh', matcher: '^bash$', timeoutSec: 7 }],
        },
      }));

      const installed = runHooksInstall([
        '--host', 'copilot', '--profile', 'full', '--project-dir', projectDir,
      ], { cwd: projectDir, hookDir });
      expect(installed.exitCode).toBe(0);

      const settings = JSON.parse(readFileSync(settingsPath, 'utf8')) as Record<string, any>;
      expect(settings).toMatchObject({
        version: 1,
        disableAllHooks: false,
        ownerMetadata: { team: 'platform' },
      });
      expect(settings.hooks.preToolUse[0]).toEqual({
        type: 'command', command: './scripts/unrelated.sh', matcher: '^bash$', timeoutSec: 7,
      });
      const awareness = settings.hooks.preToolUse.find((item: Record<string, unknown>) =>
        String(item.command).includes('hook-runner.mjs'));
      expect(awareness).toMatchObject({
        type: 'command',
        matcher: '^(?:create|edit|str_replace_editor|apply_patch)$',
        timeoutSec: 20,
      });
      expect(awareness.command).toContain('pre-edit --host copilot --skill-root');
      expect(awareness).not.toHaveProperty('hooks');
      expect(awareness).not.toHaveProperty('timeout');

      const checked = runHooksInstall([
        '--host', 'copilot', '--profile', 'full', '--project-dir', projectDir, '--check', '--strict',
      ], { cwd: projectDir, hookDir });
      expect(checked.exitCode).toBe(0);
      expect(checked.payload).toMatchObject({
        ok: true,
        action: 'check',
        installed: { host: 'copilot' },
      });

      const second = runHooksInstall([
        '--host', 'copilot', '--profile', 'full', '--project-dir', projectDir,
      ], { cwd: projectDir, hookDir });
      expect(second.payload).toMatchObject({ changed: false });

      const removed = runHooksInstall([
        '--host', 'copilot', '--profile', 'full', '--project-dir', projectDir, '--remove',
      ], { cwd: projectDir, hookDir });
      expect(removed.exitCode).toBe(0);
      const afterRemove = JSON.parse(readFileSync(settingsPath, 'utf8')) as Record<string, any>;
      expect(afterRemove.ownerMetadata).toEqual({ team: 'platform' });
      expect(afterRemove.hooks.preToolUse).toEqual([
        { type: 'command', command: './scripts/unrelated.sh', matcher: '^bash$', timeoutSec: 7 },
      ]);
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

  it('rejects unsupported global GitHub Copilot installation instead of writing the wrong layout', () => {
    const { projectDir, hookDir } = fixture();
    try {
      const result = runHooksInstall(['--host', 'copilot', '--global'], {
        cwd: projectDir,
        homeDir: projectDir,
        hookDir,
      });
      expect(result.exitCode).toBe(1);
      expect(result.payload).toMatchObject({
        ok: false,
        error: 'GitHub Copilot hook installation currently supports --project-dir only',
      });
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

  it('installs Gemini CLI hooks in settings.json and preserves unrelated settings and hooks', () => {
    const { projectDir, hookDir } = fixture();
    const settingsPath = resolve(projectDir, '.gemini/settings.json');
    try {
      mkdirSync(resolve(projectDir, '.gemini'), { recursive: true });
      writeFileSync(settingsPath, JSON.stringify({
        general: { vimMode: true },
        hooksConfig: { notifications: false },
        hooks: {
          BeforeTool: [{ matcher: '^read_file$', hooks: [{ type: 'command', command: './read-audit.sh', timeout: 1234 }] }],
        },
      }));

      const installed = runHooksInstall([
        '--host', 'gemini', '--profile', 'full', '--project-dir', projectDir,
      ], { cwd: projectDir, hookDir });
      expect(installed.exitCode).toBe(0);

      const settings = JSON.parse(readFileSync(settingsPath, 'utf8')) as Record<string, any>;
      expect(settings.general).toEqual({ vimMode: true });
      expect(settings.hooksConfig).toEqual({ notifications: false });
      expect(settings.hooks.BeforeTool[0]).toEqual({
        matcher: '^read_file$', hooks: [{ type: 'command', command: './read-audit.sh', timeout: 1234 }],
      });
      const awareness = settings.hooks.BeforeTool.find((item: Record<string, any>) =>
        item.hooks?.some((hook: Record<string, unknown>) => String(hook.command).includes('hook-runner.mjs')));
      expect(awareness).toMatchObject({
        matcher: '^(?:write_file|replace|run_shell_command)$',
        hooks: [{ type: 'command', timeout: 20_000 }],
      });
      expect(awareness.hooks[0].command).toContain('pre-edit --host gemini --skill-root');

      const checked = runHooksInstall([
        '--host', 'gemini', '--profile', 'full', '--project-dir', projectDir, '--check', '--strict',
      ], { cwd: projectDir, hookDir });
      expect(checked.exitCode).toBe(0);
      expect(checked.payload).toMatchObject({
        ok: true,
        action: 'check',
        installed: { host: 'gemini' },
      });

      const removed = runHooksInstall([
        '--host', 'gemini', '--profile', 'full', '--project-dir', projectDir, '--remove',
      ], { cwd: projectDir, hookDir });
      expect(removed.exitCode).toBe(0);
      const afterRemove = JSON.parse(readFileSync(settingsPath, 'utf8')) as Record<string, any>;
      expect(afterRemove.general).toEqual({ vimMode: true });
      expect(afterRemove.hooksConfig).toEqual({ notifications: false });
      expect(afterRemove.hooks.BeforeTool).toEqual([
        { matcher: '^read_file$', hooks: [{ type: 'command', command: './read-audit.sh', timeout: 1234 }] },
      ]);
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
});
