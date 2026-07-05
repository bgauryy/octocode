import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const DIST_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../dist/bin');
const SCRIPT = resolve(DIST_DIR, 'extract-hook-files.js');
const HOOK_RUNNER = resolve(DIST_DIR, 'hook-runner.js');
const NODE = process.execPath;

function runScript(script: string, args: string[], payload: unknown) {
  return spawnSync(NODE, [script, ...args], {
    input: JSON.stringify(payload),
    encoding: 'utf8',
    timeout: 5000,
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
});
