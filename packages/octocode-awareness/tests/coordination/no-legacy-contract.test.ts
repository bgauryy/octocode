import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { execCli } from '../../src/coordination/cli.js';

const workspaces: string[] = [];

function workspace(): string {
  const value = mkdtempSync(join(tmpdir(), 'aw-lite-no-legacy-'));
  workspaces.push(value);
  return value;
}

afterEach(() => {
  for (const value of workspaces.splice(0)) rmSync(value, { recursive: true, force: true });
});

describe('Awareness no-legacy contract', () => {
  it.each([
    ['awarenessPlan', 'list'], ['verify', 'audit'], ['awarenessAgents', 'list'],
    ['memory', 'delete', '--memory-id', 'missing'], ['lock', 'wait', '--file', 'src/a.ts', '--wait-seconds', '1ms'], ['memory', 'recall', '--smart'],
  ] as const)('rejects removed adapter form %j without forwarding', (...args) => {
    const root = workspace();
    const result = execCli([...args, '--workspace', root, '--db', join(root, 'awareness.sqlite3')]);
    expect(result.code).toBe(1);
    expect(result.stderr).toMatch(/not owned by this adapter/);
  });

  it('does not export the obsolete Pi hook adapter', async () => {
    const api = await import('../../src/index.js');
    expect('wirePiAwarenessHooks' in api).toBe(false);
    expect('createPiAwarenessBridge' in api).toBe(false);
  });
});
