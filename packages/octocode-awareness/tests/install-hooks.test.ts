import { describe, expect, it } from 'vitest';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { hooksInstallUsage, runHooksInstall } from '../src/hooks-install.js';

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
const SKILL_INSTALL_SCRIPT = resolve(
  REPO_ROOT,
  'packages/octocode-awareness/skills/octocode-awareness/scripts/install.mjs',
);
const SKILL_SMOKE_SCRIPT = resolve(
  REPO_ROOT,
  'packages/octocode-awareness/skills/octocode-awareness/scripts/smoke-multi-agent.mjs',
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

function runInstallHooksRaw(args: string[], script = SCRIPT) {
  return spawnSync(NODE, [script, ...args], {
    encoding: 'utf8',
    timeout: 5000,
  });
}

describe('install-hooks', () => {
  it('runs the hook installer directly for help, install, strict check, and remove', () => {
    expect(hooksInstallUsage()).toContain('hooks install|check|remove');
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-direct-hooks-'));
    const hookDir = resolve(REPO_ROOT, 'packages/octocode-awareness/skills/octocode-awareness/scripts/hooks');
    try {
      const help = runHooksInstall(['--help'], { cwd: projectDir, hookDir });
      expect(help.exitCode).toBe(0);
      expect(help.text).toContain('--host codex');

      const dryRun = runHooksInstall(['--host', 'codex', '--project-dir', projectDir, '--dry-run'], { cwd: projectDir, hookDir });
      expect(dryRun.exitCode).toBe(0);
      expect(dryRun.payload).toMatchObject({ action: 'dry-run', host: 'codex', changed: true });

      const installed = runHooksInstall(['--host', 'codex', '--project-dir', projectDir], { cwd: projectDir, hookDir });
      expect(installed.exitCode).toBe(0);
      expect(installed.payload).toMatchObject({ action: 'install', host: 'codex' });

      const check = runHooksInstall(['--host', 'codex', '--project-dir', projectDir, '--check', '--strict'], { cwd: projectDir, hookDir });
      expect(check.exitCode).toBe(0);
      expect(check.payload).toMatchObject({ ok: true, action: 'check', strict: true });

      const remove = runHooksInstall(['--host', 'codex', '--project-dir', projectDir, '--remove', '--dry-run'], { cwd: projectDir, hookDir });
      expect(remove.exitCode).toBe(0);
      expect(remove.payload).toMatchObject({ action: 'dry-run', host: 'codex' });

      const cursor = runHooksInstall(['--host', 'cursor', '--project-dir', projectDir, '--dry-run'], { cwd: projectDir, hookDir });
      expect(cursor.exitCode).toBe(0);
      expect(cursor.payload).toMatchObject({ action: 'dry-run', host: 'cursor', changed: true });

      const globalClaude = runHooksInstall(['--host', 'claude', '--global', '--dry-run'], {
        cwd: projectDir,
        homeDir: projectDir,
        hookDir,
      });
      expect(globalClaude.exitCode).toBe(0);
      expect(globalClaude.payload).toMatchObject({ action: 'dry-run', host: 'claude' });
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

  it('reports direct installer validation failures', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-direct-hooks-fail-'));
    const hookDir = resolve(REPO_ROOT, 'packages/octocode-awareness/skills/octocode-awareness/scripts/hooks');
    try {
      expect(runHooksInstall(['--global', '--project-dir', projectDir], { cwd: projectDir, hookDir }).payload).toMatchObject({
        ok: false,
        error: 'use either --global or --project-dir, not both',
      });
      expect(runHooksInstall(['--check'], { cwd: projectDir, hookDir }).payload).toMatchObject({
        ok: false,
        error: 'hooks check requires --host claude, --host codex, or --host cursor',
      });
      expect(runHooksInstall(['--host', 'unknown'], { cwd: projectDir, hookDir }).payload).toMatchObject({
        ok: false,
        error: 'invalid --host; expected claude, codex, or cursor',
      });
      mkdirSync(resolve(projectDir, '.codex'), { recursive: true });
      writeFileSync(resolve(projectDir, '.codex/hooks.json'), '{not json');
      expect(runHooksInstall(['--host', 'codex', '--project-dir', projectDir], { cwd: projectDir, hookDir }).payload).toMatchObject({
        ok: false,
      });
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

  it('skill install script prints the hook init flow without SQLite warnings', () => {
    const result = spawnSync(NODE, [SKILL_INSTALL_SCRIPT, '--check-only'], {
      encoding: 'utf8',
      timeout: 30000,
    });
    expect(result.status, result.stderr || result.stdout).toBe(0);
    expect(result.stderr).not.toContain('ExperimentalWarning');
    const parsed = JSON.parse(result.stdout) as {
      commands: Record<string, string>;
      next_steps: string[];
    };
    expect(parsed.commands.hooks_preview_codex).toContain('hooks install --host codex');
    expect(parsed.commands.hooks_install_cursor).toContain('hooks install --host cursor');
    expect(parsed.next_steps.join('\n')).toContain('SKILL.md frontmatter is not enough');
  });

  it('skill smoke script help is quiet about node:sqlite ExperimentalWarning', () => {
    const result = spawnSync(NODE, [SKILL_SMOKE_SCRIPT, '--help'], {
      encoding: 'utf8',
      timeout: 5000,
    });
    expect(result.status, result.stderr || result.stdout).toBe(0);
    expect(result.stdout).toContain('smoke-multi-agent');
    expect(result.stderr).not.toContain('ExperimentalWarning');
  });

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

  it('requires --host for hooks check', () => {
    const result = runInstallHooksRaw(['hooks', 'check', '--compact']);
    expect(result.status).toBe(1);
    const parsed = JSON.parse(result.stdout) as { error?: string };
    expect(parsed.error).toContain('hooks check requires --host');
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

  it('strict check reports drifted hooks and install repairs them', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-codex-drift-'));
    const preEdit = resolve(
      REPO_ROOT,
      'packages/octocode-awareness/skills/octocode-awareness/scripts/hooks/pre-edit.sh',
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
        installed: { hooks: Record<string, boolean>; drifted: string[] };
      };
      expect(parsed.ok).toBe(false);
      expect(parsed.installed.hooks['PreToolUse:pre-edit.sh']).toBe(false);
      expect(parsed.installed.drifted).toContain('PreToolUse:pre-edit.sh');

      const repaired = runInstallHooks(['hooks', 'install', '--host', 'codex', '--project-dir', projectDir, '--dry-run']);
      const preToolUse = repaired.resultingSettings.hooks?.PreToolUse ?? [];
      const preEditEntries = preToolUse.filter((entry) => JSON.stringify(entry).includes('pre-edit.sh'));
      expect(preEditEntries).toHaveLength(1);
      expect(preEditEntries[0]).toMatchObject({
        matcher: 'Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch',
      });
      expect(JSON.stringify(preEditEntries[0])).toContain('"timeout":20');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

  it('strict check reports exact hooks with stale duplicate awareness entries', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-codex-duplicate-drift-'));
    const preEdit = resolve(
      REPO_ROOT,
      'packages/octocode-awareness/skills/octocode-awareness/scripts/hooks/pre-edit.sh',
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

      const check = runInstallHooksRaw(['hooks', 'check', '--host', 'codex', '--project-dir', projectDir, '--strict', '--compact']);
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
      const preEditEntries = preToolUse.filter((entry) => JSON.stringify(entry).includes('pre-edit.sh'));
      expect(preEditEntries).toHaveLength(1);
      expect(preEditEntries[0]).toMatchObject({
        matcher: 'Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch',
      });
      expect(JSON.stringify(preEditEntries[0])).toContain('"timeout":20');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
});
