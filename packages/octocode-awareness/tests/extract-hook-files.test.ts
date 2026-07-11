import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const DIST_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../out');
const SCRIPT = resolve(DIST_DIR, 'extract-hook-files.js');
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

  it('does not treat Write file bodies as apply_patch commands', () => {
    expect(extract({
      tool_name: 'Write',
      tool_input: {
        file_path: 'docs/example.md',
        content: 'documentation example:\n*** Add File: src/phantom.ts',
      },
    })).toEqual(['docs/example.md']);
  });
});
