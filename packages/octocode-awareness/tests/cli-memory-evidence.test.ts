import { expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';

it('captures and checks memory evidence through the real built CLI with lean status preserved', () => {
  const workspace = mkdtempSync(join(tmpdir(), 'awareness-memory-cli-'));
  const runner = fileURLToPath(new URL('../out/octocode-awareness.js', import.meta.url));
  const run = (...args: string[]) => {
    const result = spawnSync(process.execPath, [runner, ...args, '--db', join(workspace, 'awareness.sqlite3'), '--workspace', workspace, '--compact'],
      { encoding: 'utf8', cwd: workspace, timeout: 10000 });
    expect(result.status, result.stderr + result.stdout).toBe(0);
    return JSON.parse(result.stdout);
  };
  try {
    writeFileSync(join(workspace, 'source.ts'), 'export const value = 1;');
    writeFileSync(join(workspace, 'dependency.ts'), 'export const input = 1;');
    run('memory', 'record', '--agent-id', 'reader', '--task-context', 'source value', '--observation', 'Inspect the value contract before editing.',
      '--importance', '5', '--file', 'source.ts', '--file', 'dependency.ts', '--capture-fingerprint');
    const db = new DatabaseSync(join(workspace, 'awareness.sqlite3'), { readOnly: true });
    expect(db.prepare("SELECT event_type, COUNT(*) AS count FROM event_outbox GROUP BY event_type").all())
      .toEqual([{ event_type: 'memory.recorded', count: 1 }]);
    db.close();
    expect(run('memory', 'recall').memories[0].evidence).toMatchObject({ state: 'unknown', reason: 'unchecked' });
    expect(run('memory', 'recall', '--check-fingerprint').memories[0].evidence.state).toBe('fresh');
    writeFileSync(join(workspace, 'dependency.ts'), 'export const input = 2;');
    expect(run('memory', 'recall', '--check-fingerprint').memories[0].evidence.state).toBe('stale');
    expect(run('memory', 'recall', '--check-fingerprint=false').memories[0].evidence.state).toBe('unknown');
    expect(run('verify', 'audit', '--agent-id', 'reader').unverified_count ?? 0).toBe(0);
  } finally { rmSync(workspace, { recursive: true, force: true }); }
});
