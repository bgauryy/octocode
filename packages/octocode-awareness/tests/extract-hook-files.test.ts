import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const DIST_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../dist/bin');
const SCRIPT = resolve(DIST_DIR, 'extract-hook-files.js');
const HOOK_RUNNER = resolve(DIST_DIR, 'hook-runner.js');
const AWARENESS = resolve(DIST_DIR, 'awareness.js');
const NODE = process.execPath;

function runScript(script: string, args: string[], payload: unknown, env: Record<string, string | undefined> = {}) {
  return spawnSync(NODE, [script, ...args], {
    input: JSON.stringify(payload),
    encoding: 'utf8',
    timeout: 5000,
    env: { ...process.env, ...env },
  });
}

function extract(payload: unknown): string[] {
  const result = runScript(SCRIPT, [], payload);
  expect(result.status).toBe(0);
  return result.stdout.trim() ? result.stdout.trim().split('\n') : [];
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
        'agent-registry',
        '--action',
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
          workspace_path: workspace,
          context: 'codex-hook',
        }),
      ]));
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });
});
