import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const DIST_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../dist/bin');
const SCRIPT = resolve(DIST_DIR, 'extract-hook-files.js');
const HOOK_RUNNER = resolve(DIST_DIR, 'hook-runner.js');
const AWARENESS = resolve(DIST_DIR, 'awareness.js');
const SKILL_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness');
const HOOKS_DIR = resolve(SKILL_ROOT, 'scripts/hooks');
const NODE = process.execPath;

function runScript(script: string, args: string[], payload: unknown, env: Record<string, string | undefined> = {}, cwd?: string) {
  return spawnSync(NODE, [script, ...args], {
    input: JSON.stringify(payload),
    encoding: 'utf8',
    timeout: 5000,
    cwd,
    env: { ...process.env, ...env },
  });
}

function extract(payload: unknown): string[] {
  const result = runScript(SCRIPT, [], payload);
  expect(result.status).toBe(0);
  return result.stdout.trim() ? result.stdout.trim().split('\n') : [];
}

function runHookWrapper(name: string, payload: unknown, env: Record<string, string | undefined> = {}, cwd?: string) {
  return spawnSync(resolve(HOOKS_DIR, name), [], {
    input: JSON.stringify(payload),
    encoding: 'utf8',
    timeout: 5000,
    cwd,
    env: { ...process.env, ...env },
  });
}

describe('extract-hook-files', () => {
  it('supports Claude tool_input payloads', () => {
    expect(extract({ tool_input: { file_path: 'src/a.ts', file_paths: ['src/b.ts'] } })).toEqual(['src/a.ts', 'src/b.ts']);
  });

  it('supports Pi tool event input payloads', () => {
    expect(extract({ toolName: 'write', input: { path: 'src/pi.ts' } })).toEqual(['src/pi.ts']);
  });

  it('supports Cursor flat file payloads', () => {
    expect(extract({ event_name: 'afterFileEdit', file_path: 'src/cursor.ts' })).toEqual(['src/cursor.ts']);
  });

  it('keeps Cursor flat file payloads when input contains unrelated metadata', () => {
    expect(extract({ event_name: 'afterFileEdit', file_path: 'src/mixed.ts', input: { eventId: 'evt-1' } })).toEqual(['src/mixed.ts']);
  });

  it('supports Pi args payloads and apply_patch paths', () => {
    expect(extract({ args: { command: '*** Begin Patch\n*** Add File: src/new.ts\n*** Move to: src/moved.ts\n*** End Patch' } })).toEqual([
      'src/new.ts',
      'src/moved.ts',
    ]);
  });
});

describe('hook-runner', () => {
  it('owns hook dispatch logic outside the skill wrapper scripts', () => {
    const result = runScript(HOOK_RUNNER, ['notify-deliver'], { sessionId: 'agent-a', workspace: process.cwd() });
    expect(result.status).toBe(0);
    if (result.stdout.trim()) {
      expect(() => JSON.parse(result.stdout)).not.toThrow();
    }
  });

  it('registers hook agents before checking mailbox delivery', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-agent-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'hook-agent',
        OCTOCODE_AGENT_NAME: 'Hook Agent',
        OCTOCODE_AGENT_CONTEXT: 'codex-hook',
        OCTOCODE_NO_DIGEST: '1',
      };
      const result = runScript(HOOK_RUNNER, ['notify-deliver'], { sessionId: 'session-a', workspace }, env);
      expect(result.status).toBe(0);

      const listed = spawnSync(NODE, [
        AWARENESS,
        'agent',
        'list',
        '--workspace',
        workspace,
      ], {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, OCTOCODE_MEMORY_HOME: memoryHome },
      });
      expect(listed.status).toBe(0);
      const parsed = JSON.parse(listed.stdout) as { agents: Array<Record<string, unknown>> };
      expect(parsed.agents).toEqual(expect.arrayContaining([
        expect.objectContaining({
          agent_id: 'hook-agent',
          agent_name: 'Hook Agent',
          workspace_path: realpathSync(workspace),
          context: 'codex-hook',
        }),
      ]));
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('claims flat file_path hook payloads even when toolName is absent', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-flat-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'flat-hook-agent',
      };
      const result = runScript(
        HOOK_RUNNER,
        ['pre-edit'],
        { sessionId: 'flat-session', workspace, file_path: 'src/cursor.ts' },
        env,
      );
      expect(result.status).toBe(0);

      const status = spawnSync(NODE, [
        AWARENESS,
        'workspace',
        'status',
        '--workspace',
        workspace,
      ], {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, OCTOCODE_MEMORY_HOME: memoryHome },
      });
      expect(status.status).toBe(0);
      const parsed = JSON.parse(status.stdout) as { locks: Array<{ file_path: string; agent_id: string }> };
      expect(parsed.locks).toEqual(expect.arrayContaining([
        expect.objectContaining({
          file_path: resolve(workspace, 'src/cursor.ts'),
          agent_id: 'flat-hook-agent',
        }),
      ]));
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('claims mixed root file_path payloads even when input contains unrelated metadata', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-mixed-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'mixed-hook-agent',
      };
      const result = runScript(
        HOOK_RUNNER,
        ['pre-edit'],
        { sessionId: 'mixed-session', workspace, file_path: 'src/mixed.ts', input: { eventId: 'evt-1' } },
        env,
      );
      expect(result.status).toBe(0);

      const status = spawnSync(NODE, [
        AWARENESS,
        'workspace',
        'status',
        '--workspace',
        workspace,
      ], {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, OCTOCODE_MEMORY_HOME: memoryHome },
      });
      expect(status.status).toBe(0);
      const parsed = JSON.parse(status.stdout) as { locks: Array<{ file_path: string; agent_id: string }> };
      expect(parsed.locks).toEqual(expect.arrayContaining([
        expect.objectContaining({
          file_path: resolve(workspace, 'src/mixed.ts'),
          agent_id: 'mixed-hook-agent',
        }),
      ]));
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('post-edit releases only the correlated same-agent hook task', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-overlap-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'overlap-hook-agent',
      };
      const first = { sessionId: 'overlap-session', workspace, eventId: 'tool-1', file_path: 'src/shared.ts' };
      const second = { sessionId: 'overlap-session', workspace, eventId: 'tool-2', file_path: 'src/shared.ts' };

      expect(runScript(HOOK_RUNNER, ['pre-edit'], first, env).status).toBe(0);
      expect(runScript(HOOK_RUNNER, ['pre-edit'], second, env).status).toBe(0);

      const before = spawnSync(NODE, [AWARENESS, 'workspace', 'status', '--workspace', workspace], {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, OCTOCODE_MEMORY_HOME: memoryHome },
      });
      expect(before.status).toBe(0);
      expect((JSON.parse(before.stdout) as { locks: unknown[] }).locks).toHaveLength(2);

      expect(runScript(HOOK_RUNNER, ['post-edit'], first, env).status).toBe(0);
      const afterFirst = spawnSync(NODE, [AWARENESS, 'workspace', 'status', '--workspace', workspace], {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, OCTOCODE_MEMORY_HOME: memoryHome },
      });
      expect(afterFirst.status).toBe(0);
      expect((JSON.parse(afterFirst.stdout) as { locks: unknown[] }).locks).toHaveLength(1);

      expect(runScript(HOOK_RUNNER, ['post-edit'], second, env).status).toBe(0);
      const afterSecond = spawnSync(NODE, [AWARENESS, 'workspace', 'status', '--workspace', workspace], {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, OCTOCODE_MEMORY_HOME: memoryHome },
      });
      expect(afterSecond.status).toBe(0);
      expect((JSON.parse(afterSecond.stdout) as { locks: unknown[] }).locks).toEqual([]);
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });
});

describe('hook wrapper scripts', () => {
  it('pre-edit.sh and post-edit.sh dispatch through hook-runner.mjs', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-wrapper-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'wrapper-agent',
      };
      const payload = { sessionId: 'wrapper-session', workspace, file_path: 'src/wrapped.ts' };

      const pre = runHookWrapper('pre-edit.sh', payload, env, workspace);
      expect(pre.status, pre.stderr).toBe(0);

      const locked = spawnSync(NODE, [AWARENESS, 'workspace', 'status', '--workspace', workspace], {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, OCTOCODE_MEMORY_HOME: memoryHome },
      });
      expect(locked.status).toBe(0);
      const lockedParsed = JSON.parse(locked.stdout) as { locks: Array<{ file_path: string; agent_id: string }> };
      expect(lockedParsed.locks).toEqual(expect.arrayContaining([
        expect.objectContaining({
          file_path: resolve(workspace, 'src/wrapped.ts'),
          agent_id: 'wrapper-agent',
        }),
      ]));

      const post = runHookWrapper('post-edit.sh', payload, env, workspace);
      expect(post.status, post.stderr).toBe(0);

      const released = spawnSync(NODE, [AWARENESS, 'workspace', 'status', '--workspace', workspace], {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, OCTOCODE_MEMORY_HOME: memoryHome },
      });
      expect(released.status).toBe(0);
      const releasedParsed = JSON.parse(released.stdout) as { locks: unknown[] };
      expect(releasedParsed.locks).toEqual([]);
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('harness-guard.sh passes OCTOCODE_SKILL_ROOT to the runner', () => {
    const result = runHookWrapper(
      'harness-guard.sh',
      { tool_name: 'Edit', tool_input: { file_path: 'SKILL.md' } },
      { OCTOCODE_ALLOW_HARNESS_APPLY: undefined },
      SKILL_ROOT,
    );
    expect(result.status).toBe(2);
    expect(result.stderr).toContain('editing the skill itself is gated');
  });

  it('hook wrappers warn when hook-runner.mjs is missing', () => {
    const tempRoot = mkdtempSync(join(tmpdir(), 'octocode-missing-runner-'));
    const tempHooks = join(tempRoot, 'scripts', 'hooks');
    mkdirSync(tempHooks, { recursive: true });
    try {
      cpSync(resolve(HOOKS_DIR, 'pre-edit.sh'), join(tempHooks, 'pre-edit.sh'));
      const result = spawnSync(join(tempHooks, 'pre-edit.sh'), [], {
        input: JSON.stringify({ file_path: 'src/missing-runner.ts' }),
        encoding: 'utf8',
        timeout: 5000,
      });
      expect(result.status).toBe(0);
      expect(result.stderr).toContain('missing hook runner');
      expect(result.stderr).toContain('pre-edit hook skipped');
    } finally {
      rmSync(tempRoot, { recursive: true, force: true });
    }
  });
});

describe('hook-runner harness-guard containment', () => {
  // Exercises the harness-guard CLI dispatch end-to-end (payload extraction +
  // path resolution + containment check), which previously had no integration
  // test — hiding a relative-path traversal bypass of the self-edit gate.
  function guard(skillRoot: string | undefined, files: string[], cwd: string, extraEnv: Record<string, string | undefined> = {}) {
    return runScript(
      HOOK_RUNNER,
      ['harness-guard'],
      { tool_name: 'Edit', tool_input: { file_paths: files } },
      { OCTOCODE_SKILL_ROOT: skillRoot, OCTOCODE_ALLOW_HARNESS_APPLY: undefined, ...extraEnv },
      cwd,
    );
  }

  it('is a no-op when OCTOCODE_SKILL_ROOT is unset', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    try {
      expect(guard(undefined, ['SKILL.md'], tmp).status).toBe(0);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('allows edits resolving outside the skill root', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    const skillRoot = join(tmp, 'skill');
    const project = join(tmp, 'project');
    mkdirSync(skillRoot, { recursive: true });
    mkdirSync(project, { recursive: true });
    try {
      expect(guard(skillRoot, ['notes.txt'], project).status).toBe(0);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('gates absolute edits inside the skill root', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    const skillRoot = join(tmp, 'skill');
    mkdirSync(skillRoot, { recursive: true });
    try {
      const result = guard(skillRoot, [join(skillRoot, 'SKILL.md')], tmp);
      expect(result.status).toBe(2);
      expect(result.stderr).toContain('OCTOCODE_ALLOW_HARNESS_APPLY');
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('gates relative traversal edits that resolve inside the skill root', () => {
    // The bypass: cwd is outside the skill root, but a `../` path resolves back
    // in. A textual prefix check misses this; a normalized check must catch it.
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    const skillRoot = join(tmp, 'skill');
    const project = join(tmp, 'project');
    mkdirSync(skillRoot, { recursive: true });
    mkdirSync(project, { recursive: true });
    try {
      const result = guard(skillRoot, ['../skill/SKILL.md'], project);
      expect(result.status).toBe(2);
      expect(result.stderr).toContain('OCTOCODE_ALLOW_HARNESS_APPLY');
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('does not gate a sibling directory sharing the root name prefix', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    const skillRoot = join(tmp, 'skill');
    const sibling = join(tmp, 'skill-sibling');
    mkdirSync(skillRoot, { recursive: true });
    mkdirSync(sibling, { recursive: true });
    try {
      expect(guard(skillRoot, [join(sibling, 'x.ts')], tmp).status).toBe(0);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});
