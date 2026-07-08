import { describe, expect, it } from 'vitest';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(TEST_DIR, '../../..');
const SCRIPT = resolve(
  REPO_ROOT,
  'packages/octocode-awareness/dist/bin/awareness.js',
);
const SKILL_SCRIPT = resolve(
  REPO_ROOT,
  'packages/octocode-awareness/skills/octocode-awareness/scripts/awareness.mjs',
);
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
    resultingSettings: { version?: number; hooks?: Record<string, Array<Record<string, unknown>>> };
  };
}

describe('install-hooks', () => {
  it('rejects host shortcut aliases', () => {
    const result = spawnSync(NODE, [SCRIPT, 'hooks', 'install', '--codex', '--dry-run'], {
      encoding: 'utf8',
      timeout: 5000,
    });
    expect(result.status).toBe(1);
    const parsed = JSON.parse(result.stdout) as { error?: string; known_flags?: string[] };
    expect(parsed.error).toContain('unknown flag');
    expect(parsed.known_flags).toContain('--host');
    expect(parsed.known_flags).not.toContain('--codex');
  });

  it('generated skill CLI resolves hook paths from its own scripts directory', () => {
    expect(existsSync(SKILL_SCRIPT), 'generated awareness.mjs must exist after build').toBe(true);
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-skill-hooks-'));
    try {
      const result = runInstallHooks(['hooks', 'install', '--host', 'codex', '--project-dir', projectDir, '--dry-run'], SKILL_SCRIPT);
      const serialized = JSON.stringify(result.resultingSettings);
      expect(serialized).toContain('/skills/octocode-awareness/scripts/hooks/pre-edit.sh');
      expect(serialized).not.toContain('/skills/skills/');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

  it('previews Codex hooks in .codex/hooks.json without unsupported SessionEnd', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-codex-hooks-'));
    try {
      const result = runInstallHooks(['hooks', 'install', '--host', 'codex', '--project-dir', projectDir, '--dry-run']);

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
      const result = runInstallHooks(['hooks', 'install', '--host', 'claude', '--project-dir', projectDir, '--dry-run']);

      expect(result.host).toBe('claude');
      expect(result.settingsPath).toBe(resolve(projectDir, '.claude/settings.json'));
      expect(result.resultingSettings.hooks).toHaveProperty('SessionEnd');
      expect(result.resultingSettings.hooks).not.toHaveProperty('PreCompact');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

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
        'stop',
        'subagentStop',
        'sessionEnd',
        'preCompact',
        'sessionStart',
      ]);
      expect(result.resultingSettings.hooks?.preToolUse?.[0]).toMatchObject({
        command: expect.stringContaining('pre-edit.sh'),
        timeout: 20,
        matcher: expect.stringContaining('Write'),
      });
      expect(result.resultingSettings.hooks?.preToolUse?.[0]).not.toHaveProperty('hooks');
      expect(JSON.stringify(result.resultingSettings)).not.toContain('CLAUDE_PROJECT_DIR');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

  it('removes Cursor awareness hooks without deleting unrelated flat hooks', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-cursor-remove-'));
    const unrelated = '/tmp/unrelated-cursor-hook.sh';
    const preEdit = resolve(
      REPO_ROOT,
      'packages/octocode-awareness/skills/octocode-awareness/scripts/hooks/pre-edit.sh',
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
});
