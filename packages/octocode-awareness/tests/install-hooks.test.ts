import { describe, expect, it } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(TEST_DIR, '../../..');
const SCRIPT = resolve(
  REPO_ROOT,
  'packages/octocode-awareness/skills/octocode-awareness/scripts/install-hooks.mjs',
);
const NODE = process.execPath;

function runInstallHooks(args: string[]) {
  const result = spawnSync(NODE, [SCRIPT, ...args], {
    encoding: 'utf8',
    timeout: 5000,
  });
  expect(result.status).toBe(0);
  return JSON.parse(result.stdout) as {
    host: string;
    settingsPath: string;
    resultingSettings: { hooks?: Record<string, unknown> };
  };
}

describe('install-hooks', () => {
  it('previews Codex hooks in .codex/hooks.json without unsupported SessionEnd', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-codex-hooks-'));
    try {
      const result = runInstallHooks(['--host', 'codex', '--project-dir', projectDir, '--dry-run']);

      expect(result.host).toBe('codex');
      expect(result.settingsPath).toBe(resolve(projectDir, '.codex/hooks.json'));
      expect(Object.keys(result.resultingSettings.hooks ?? {})).toEqual([
        'PreToolUse',
        'PostToolUse',
        'Stop',
        'SubagentStop',
        'PreCompact',
        'UserPromptSubmit',
      ]);
      expect(result.resultingSettings.hooks).not.toHaveProperty('SessionEnd');
      expect(JSON.stringify(result.resultingSettings)).not.toContain('CLAUDE_PROJECT_DIR');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

  it('keeps Claude hooks in .claude/settings.json with SessionEnd', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-claude-hooks-'));
    try {
      const result = runInstallHooks(['--host', 'claude', '--project-dir', projectDir, '--dry-run']);

      expect(result.host).toBe('claude');
      expect(result.settingsPath).toBe(resolve(projectDir, '.claude/settings.json'));
      expect(result.resultingSettings.hooks).toHaveProperty('SessionEnd');
      expect(result.resultingSettings.hooks).not.toHaveProperty('PreCompact');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
});
