import { describe, expect, it } from 'vitest';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { hooksInstallUsage, runHooksInstall } from '../src/hooks-install.js';
const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(TEST_DIR, '../../..');
const SCRIPT = resolve(REPO_ROOT, 'packages/octocode-awareness/out/octocode-awareness.js');
const INDEX_SCRIPT = resolve(REPO_ROOT, 'packages/octocode-awareness/out/index.js');
const SKILL_SCRIPT = resolve(REPO_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs');
const SKILL_INSTALL_SCRIPT = resolve(REPO_ROOT, 'skills/octocode-awareness/scripts/install.mjs');
const SKILL_SMOKE_SCRIPT = resolve(REPO_ROOT, 'skills/octocode-awareness/scripts/smoke-multi-agent.mjs');
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
it('serializes concurrent installers and never exposes partial JSON', { timeout: 30_000 }, async () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-concurrent-hooks-'));
    const settingsPath = resolve(projectDir, '.codex/hooks.json');
    try {
      mkdirSync(resolve(projectDir, '.codex'), { recursive: true });
      writeFileSync(settingsPath, JSON.stringify({ unrelated: 'x'.repeat(8 * 1024 * 1024) }));

      let parseErrors = 0;
      const poll = setInterval(() => {
        try {
          JSON.parse(readFileSync(settingsPath, 'utf8'));
        } catch {
          parseErrors += 1;
        }
      }, 1);

      const results = await Promise.all(Array.from({ length: 12 }, () =>
        new Promise<{ code: number | null; stderr: string }>((resolveChild) => {
          const child = spawn(NODE, [
            SCRIPT, 'hooks', 'install', '--host', 'codex', '--project-dir', projectDir, '--compact',
          ], { stdio: ['ignore', 'ignore', 'pipe'] });
          let stderr = '';
          child.stderr.setEncoding('utf8');
          child.stderr.on('data', (chunk: string) => { stderr += chunk; });
          child.on('close', (code) => resolveChild({ code, stderr }));
        }),
      ));
      clearInterval(poll);

      expect(
        results.map((result) => result.code),
        results.map((result) => result.stderr).filter(Boolean).join('\n'),
      ).toEqual(results.map(() => 0));
      expect(parseErrors).toBe(0);
      const finalSettings = JSON.parse(readFileSync(settingsPath, 'utf8')) as {
        unrelated?: string;
        hooks?: Record<string, Array<Record<string, unknown>>>;
      };
      expect(finalSettings.unrelated).toHaveLength(8 * 1024 * 1024);
      expect(finalSettings.hooks?.PreToolUse).toHaveLength(1);
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
it('runs the hook installer directly for help, install, strict check, and remove', () => {
    expect(hooksInstallUsage()).toContain('hooks install|check|remove');
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-direct-hooks-'));
    const hookDir = resolve(REPO_ROOT, 'skills/octocode-awareness/scripts/hooks');
    try {
      const help = runHooksInstall(['--help'], { cwd: projectDir, hookDir });
      expect(help.exitCode).toBe(0);
      expect(help.text).toContain('--host codex');
      expect(help.text).toContain('wirePiAwarenessHooks');

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
    const hookDir = resolve(REPO_ROOT, 'skills/octocode-awareness/scripts/hooks');
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
    const result = spawnSync(NODE, [SKILL_INSTALL_SCRIPT], {
      encoding: 'utf8',
      timeout: 30000,
    });
    expect(result.status, result.stderr || result.stdout).toBe(0);
    expect(result.stderr).not.toContain('ExperimentalWarning');
    const parsed = JSON.parse(result.stdout) as {
      scriptsDir: string;
      commands: Record<string, string>;
      next_steps: string[];
      runtime: { dependencies: string; writes: boolean };
    };
    expect(parsed.runtime).toEqual({ dependencies: 'bundled', writes: false });
    expect(parsed.commands).not.toHaveProperty('hooks_preview_claude');
    expect(parsed.commands).not.toHaveProperty('hooks_install_claude');
    expect(parsed.commands).not.toHaveProperty('hooks_check_claude');
    expect(parsed.commands.hooks_preview_codex).toContain('hooks install --host codex');
    expect(parsed.commands.hooks_check_codex).toContain('hooks check --host codex');
    expect(parsed.commands.hooks_install_cursor).toContain('hooks install --host cursor');
    expect(parsed.commands.hooks_check_cursor).toContain('hooks check --host cursor');
    expect(parsed.commands.pi_bridge).toContain('wirePiAwarenessHooks');
    for (const key of ['schema', 'awareness', 'init', 'attend', 'hooks_preview_codex', 'hooks_preview_cursor']) {
      expect(parsed.commands[key]).toContain(parsed.scriptsDir);
    }
    const arbitraryCwd = mkdtempSync(resolve(tmpdir(), 'octocode-install-command-cwd-'));
    try {
      const schema = spawnSync('/bin/sh', ['-c', parsed.commands.schema!], {
        cwd: arbitraryCwd,
        encoding: 'utf8',
        timeout: 5000,
      });
      expect(schema.status, schema.stderr || schema.stdout).toBe(0);
    } finally {
      rmSync(arbitraryCwd, { recursive: true, force: true });
    }
    const nextSteps = parsed.next_steps.join('\n');
    expect(nextSteps).toContain('stable OCTOCODE_AGENT_ID');
    expect(nextSteps).toContain('Claude skill frontmatter');
    expect(nextSteps).toContain('do not also install');
    expect(nextSteps).toContain('Codex and Cursor');
    expect(nextSteps).not.toContain('Claude, Codex, and Cursor project hooks');
    expect(parsed.next_steps.join('\n')).toContain('For Pi: do not run shell hook install');
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
it('package CLI and library imports selectively suppress node:sqlite ExperimentalWarning', () => {
    const cli = spawnSync(NODE, [SCRIPT, '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(cli.status, cli.stderr || cli.stdout).toBe(0);
    expect(cli.stderr).not.toContain('ExperimentalWarning');

    const library = spawnSync(NODE, [
      '--input-type=module', '-e',
      `await import(${JSON.stringify(INDEX_SCRIPT)}); process.emitWarning('awareness-warning-probe'); await new Promise((resolve) => setImmediate(resolve))`,
    ], { encoding: 'utf8', timeout: 5000 });
    expect(library.status, library.stderr || library.stdout).toBe(0);
    expect(library.stderr).not.toContain('ExperimentalWarning');
    expect(library.stderr).toContain('awareness-warning-probe');
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
      expect(serialized).toContain('/skills/octocode-awareness/scripts/hook-runner.mjs');
      expect(serialized).toContain(' pre-edit --host codex --skill-root ');
      expect(serialized).not.toContain('/skills/skills/');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
it('project hook configs prefer the canonical direct runner and preserve Claude project expansion', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-project-runner-'));
    const canonicalHookDir = resolve(projectDir, 'skills/octocode-awareness/scripts/hooks');
    const transientHookDir = resolve(projectDir, 'packages/octocode-awareness/out/skills/octocode-awareness/scripts/hooks');
    try {
      mkdirSync(canonicalHookDir, { recursive: true });
      mkdirSync(transientHookDir, { recursive: true });
      writeFileSync(resolve(canonicalHookDir, '..', 'hook-runner.mjs'), '#!/usr/bin/env node\n');
      writeFileSync(resolve(transientHookDir, '..', 'hook-runner.mjs'), '#!/usr/bin/env node\n');

      for (const host of ['codex', 'cursor'] as const) {
        const result = runHooksInstall(['--host', host, '--project-dir', projectDir, '--dry-run'], {
          cwd: projectDir,
          hookDir: transientHookDir,
        });
        const serialized = JSON.stringify(result.payload);
        expect(serialized).toContain('/skills/octocode-awareness/scripts/hook-runner.mjs');
        expect(serialized).not.toContain('/packages/octocode-awareness/out/');
        expect(serialized).toContain(`--host ${host}`);
        expect(serialized).toContain('--skill-root');
      }

      const claude = runHooksInstall(['--host', 'claude', '--project-dir', projectDir, '--dry-run'], {
        cwd: projectDir,
        hookDir: transientHookDir,
      });
      const serializedClaude = JSON.stringify(claude.payload);
      expect(serializedClaude).toContain('${CLAUDE_PROJECT_DIR}/skills/octocode-awareness/scripts/hook-runner.mjs');
      expect(serializedClaude).toContain('${CLAUDE_PROJECT_DIR}/skills/octocode-awareness');
      expect(serializedClaude).not.toContain('\\\\${CLAUDE_PROJECT_DIR}');

      rmSync(resolve(canonicalHookDir, '..', 'hook-runner.mjs'));
      const fallback = runHooksInstall(['--host', 'codex', '--project-dir', projectDir, '--dry-run'], {
        cwd: projectDir,
        hookDir: transientHookDir,
      });
      expect(JSON.stringify(fallback.payload)).toContain('/packages/octocode-awareness/out/');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
it('installed Codex hook commands run when Node is absent from PATH', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-codex-node-path-'));
    const memoryHome = mkdtempSync(resolve(tmpdir(), 'octocode-codex-node-memory-'));
    try {
      const preview = runInstallHooks([
        'hooks', 'install', '--host', 'codex', '--project-dir', projectDir, '--dry-run',
      ]);
      const preToolUse = preview.resultingSettings.hooks?.PreToolUse?.[0] as {
        hooks?: Array<{ command?: string }>;
      };
      const command = preToolUse.hooks?.[0]?.command ?? '';
      expect(command).toContain(`"${NODE}"`);
      expect(command).toContain('hook-runner.mjs" pre-edit --host codex --skill-root');
      expect(command).not.toContain('OCTOCODE_NODE_BIN=');

      const result = spawnSync(command, {
        shell: true,
        cwd: projectDir,
        input: JSON.stringify({ workspace: projectDir, eventId: 'no-node-path', file_path: 'src/a.ts' }),
        encoding: 'utf8',
        timeout: 5000,
        env: {
          ...process.env,
          PATH: '/usr/bin:/bin',
          OCTOCODE_MEMORY_HOME: memoryHome,
          OCTOCODE_AGENT_ID: 'no-node-path-agent',
        },
      });
      expect(result.status, result.stderr || result.stdout).toBe(0);
      expect(result.stderr).not.toContain('node: not found');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });
it('previews Codex hooks in .codex/hooks.json without unsupported SessionEnd', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-codex-hooks-'));
    try {
      const result = runInstallHooks(['hooks', 'install', '--host', 'codex', '--project-dir', projectDir, '--dry-run']);

      expect(result.host).toBe('codex');
      expect(result.settingsPath).toBe(resolve(projectDir, '.codex/hooks.json'));
      expect(Object.keys(result.resultingSettings.hooks ?? {})).toEqual([
        'SessionStart',
        'PreToolUse',
        'PostToolUse',
        'SubagentStart',
        'Stop',
        'SubagentStop',
        'PreCompact',
        'UserPromptSubmit',
      ]);
      expect(result.resultingSettings.hooks).not.toHaveProperty('SessionEnd');
      const preCompact = JSON.stringify(result.resultingSettings.hooks?.PreCompact);
      expect(preCompact).toContain('hook-runner.mjs');
      expect(preCompact).toContain(' session-compact --host codex --skill-root ');
      expect(preCompact).not.toContain(' session-end --host codex ');
      expect(JSON.stringify(result.resultingSettings)).not.toContain('CLAUDE_PROJECT_DIR');
      const preToolUse = result.resultingSettings.hooks?.PreToolUse ?? [];
      expect(preToolUse).toHaveLength(1);
      expect(JSON.stringify(preToolUse[0])).toContain('hook-runner.mjs');
      expect(JSON.stringify(preToolUse[0])).toContain(' pre-edit --host codex --skill-root ');
      expect(JSON.stringify(preToolUse)).not.toContain(' harness-guard --host codex ');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
it('removes misplaced legacy Awareness hooks while preserving unrelated hooks', () => {
    const projectDir = mkdtempSync(resolve(tmpdir(), 'octocode-stale-event-hooks-'));
    const settingsPath = resolve(projectDir, '.codex/hooks.json');
    try {
      mkdirSync(dirname(settingsPath), { recursive: true });
      writeFileSync(settingsPath, JSON.stringify({
        hooks: {
          PreCompact: [
            { hooks: [{ type: 'command', command: 'node /old/octocode-awareness/scripts/hook-runner.mjs session-end --host codex', timeout: 20 }] },
            { hooks: [{ type: 'command', command: '/tmp/unrelated.sh', timeout: 20 }] },
          ],
        },
      }));

      const installed = runInstallHooks(['hooks', 'install', '--host', 'codex', '--project-dir', projectDir, '--dry-run']);
      const serializedInstall = JSON.stringify(installed.resultingSettings);
      expect(serializedInstall).not.toContain('session-end');
      expect(serializedInstall).toContain('session-compact');
      expect(serializedInstall).toContain('/tmp/unrelated.sh');

      const removed = runInstallHooks(['hooks', 'remove', '--host', 'codex', '--project-dir', projectDir, '--dry-run']);
      const serializedRemove = JSON.stringify(removed.resultingSettings);
      expect(serializedRemove).not.toContain('octocode-awareness');
      expect(serializedRemove).toContain('/tmp/unrelated.sh');
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
      expect(result.resultingSettings.hooks).toHaveProperty('PreCompact');
      expect(result.resultingSettings.hooks).toHaveProperty('PostToolUseFailure');
      expect(result.resultingSettings.hooks).toHaveProperty('SubagentStart');
      const preCompact = JSON.stringify(result.resultingSettings.hooks?.PreCompact);
      expect(preCompact).toContain('hook-runner.mjs');
      expect(preCompact).toContain(' session-compact --host claude --skill-root ');
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

});
