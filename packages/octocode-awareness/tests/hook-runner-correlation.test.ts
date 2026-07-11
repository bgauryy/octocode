import { spawn, spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import {
  existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { removeStaleHookRunStateLock } from '../bin/hook-run-state.js';

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = resolve(TEST_DIR, '..');
const REPO_ROOT = resolve(PACKAGE_ROOT, '../..');
const SOURCE_RUNNER = resolve(PACKAGE_ROOT, 'bin/hook-runner-entry.ts');
const SOURCE_LIFECYCLE = resolve(PACKAGE_ROOT, 'bin/hook-lifecycle.ts');
const TSX_CLI = resolve(REPO_ROOT, 'node_modules/tsx/dist/cli.mjs');

function hookEnv(memoryHome: string): NodeJS.ProcessEnv {
  return {
    ...process.env,
    NODE_NO_WARNINGS: '1',
    OCTOCODE_MEMORY_HOME: memoryHome,
    OCTOCODE_AGENT_ID: 'correlation-agent',
  };
}

function runHook(
  command: 'pre-edit' | 'post-edit',
  payload: Record<string, unknown>,
  memoryHome: string,
  cwd: string,
) {
  return spawnSync(process.execPath, [TSX_CLI, SOURCE_RUNNER, command], {
    cwd,
    env: hookEnv(memoryHome),
    input: JSON.stringify(payload),
    encoding: 'utf8',
    timeout: 20_000,
  });
}

function runHookAsync(
  command: 'pre-edit' | 'post-edit',
  payload: Record<string, unknown>,
  memoryHome: string,
  cwd: string,
): Promise<{ code: number | null; stderr: string }> {
  return new Promise((resolveResult, reject) => {
    const child = spawn(process.execPath, [TSX_CLI, SOURCE_RUNNER, command], {
      cwd,
      env: hookEnv(memoryHome),
      stdio: ['pipe', 'ignore', 'pipe'],
    });
    let stderr = '';
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.on('error', reject);
    child.on('close', (code) => resolveResult({ code, stderr }));
    child.stdin.end(JSON.stringify(payload));
  });
}

function stateJsonFiles(memoryHome: string): string[] {
  const stateDir = join(memoryHome, 'hook-state', 'runs');
  return readdirSync(stateDir).filter((file) => file.endsWith('.json'));
}

describe('shell hook correlation state', () => {
  it('does not steal a fresh lock before its owner PID is written', () => {
    const root = mkdtempSync(join(tmpdir(), 'octocode-hook-lock-creation-'));
    const lockFile = join(root, 'fresh.lock');
    try {
      writeFileSync(lockFile, '');
      expect(removeStaleHookRunStateLock(lockFile)).toBe(false);
      expect(existsSync(lockFile)).toBe(true);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('drops a non-active stale entry before consuming a later same-key edit', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-correlation-invalid-'));
    const workspace = join(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    const payload = { workspace, eventId: 'reused-event', file_path: 'src/a.ts' };
    try {
      const first = runHook('pre-edit', payload, memoryHome, workspace);
      expect(first.status, first.stderr).toBe(0);

      const dbPath = join(memoryHome, 'awareness.sqlite3');
      const db = new DatabaseSync(dbPath);
      const firstRun = db.prepare('SELECT run_id FROM task_runs').get() as { run_id: string };
      const now = new Date().toISOString();
      db.prepare('UPDATE run_files SET ended_at = ?, expires_at = ? WHERE run_id = ?').run(now, now, firstRun.run_id);
      db.prepare("UPDATE task_runs SET status = 'PENDING', updated_at = ? WHERE run_id = ?").run(now, firstRun.run_id);
      db.close();

      const second = runHook('pre-edit', payload, memoryHome, workspace);
      expect(second.status, second.stderr).toBe(0);
      const post = runHook('post-edit', payload, memoryHome, workspace);
      expect(post.status, post.stderr).toBe(0);

      const inspect = new DatabaseSync(dbPath);
      expect((inspect.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status = 'ACTIVE'").get() as { c: number }).c).toBe(0);
      expect((inspect.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status = 'PENDING'").get() as { c: number }).c).toBe(2);
      expect(stateJsonFiles(memoryHome)).toHaveLength(0);
      inspect.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('expires an aged entry instead of letting it consume a later same-key post-edit', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-correlation-expired-'));
    const workspace = join(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    const payload = { workspace, eventId: 'aged-event', file_path: 'src/a.ts' };
    try {
      const first = runHook('pre-edit', payload, memoryHome, workspace);
      expect(first.status, first.stderr).toBe(0);

      const stateDir = join(memoryHome, 'hook-state', 'runs');
      const [stateFile] = stateJsonFiles(memoryHome);
      const entries = JSON.parse(readFileSync(join(stateDir, stateFile!), 'utf8')) as Array<Record<string, unknown>>;
      entries[0]!['createdAt'] = '2000-01-01T00:00:00.000Z';
      writeFileSync(join(stateDir, stateFile!), JSON.stringify(entries));

      const second = runHook('pre-edit', payload, memoryHome, workspace);
      expect(second.status, second.stderr).toBe(0);
      const post = runHook('post-edit', payload, memoryHome, workspace);
      expect(post.status, post.stderr).toBe(0);

      const db = new DatabaseSync(join(memoryHome, 'awareness.sqlite3'));
      expect((db.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status = 'ACTIVE'").get() as { c: number }).c).toBe(1);
      expect((db.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status = 'PENDING'").get() as { c: number }).c).toBe(1);
      expect(stateJsonFiles(memoryHome)).toHaveLength(0);
      db.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('serializes concurrent same-key pre/post correlation without losing runs', { timeout: 60_000 }, async () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-correlation-concurrent-'));
    const workspace = join(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    const payload = { workspace, eventId: 'parallel-event', file_path: 'src/shared.ts' };
    try {
      const starts = await Promise.all(Array.from({ length: 3 }, () => (
        runHookAsync('pre-edit', payload, memoryHome, workspace)
      )));
      for (const start of starts) expect(start.code, start.stderr).toBe(0);

      const before = new DatabaseSync(join(memoryHome, 'awareness.sqlite3'));
      expect((before.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status = 'ACTIVE'").get() as { c: number }).c).toBe(3);
      before.close();

      const finishes = await Promise.all(Array.from({ length: 3 }, () => (
        runHookAsync('post-edit', payload, memoryHome, workspace)
      )));
      for (const finish of finishes) expect(finish.code, finish.stderr).toBe(0);

      const after = new DatabaseSync(join(memoryHome, 'awareness.sqlite3'));
      expect(
        (after.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status = 'PENDING'").get() as { c: number }).c,
        finishes.map((finish) => finish.stderr).filter(Boolean).join('\n'),
      ).toBe(3);
      expect((after.prepare("SELECT COUNT(*) AS c FROM run_files WHERE ended_at IS NULL").get() as { c: number }).c).toBe(0);
      expect(stateJsonFiles(memoryHome)).toHaveLength(0);
      after.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('writes the scoped preview marker only after a dry-run digest succeeds', () => {
    const source = readFileSync(SOURCE_LIFECYCLE, 'utf8');
    const digestCall = source.indexOf('const preview = digest(database, {');
    const dryRun = source.indexOf('dry_run: true', digestCall);
    const markerWrite = source.indexOf("writeFileSync(markerPath, String(now), 'utf8');", digestCall);
    expect(source).toContain('const memoryHome = dirname(resolveDbPath(null));');
    expect(source).toContain('.last-digest-preview-${scopeHash}-epoch-ms');
    expect(digestCall).toBeGreaterThanOrEqual(0);
    expect(dryRun).toBeGreaterThan(digestCall);
    expect(markerWrite).toBeGreaterThan(dryRun);
  });
});
