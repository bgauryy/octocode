/**
 * audit-2026-07-10-fixes.test.ts — regression coverage for the 2026-07-10
 * hands-on audit findings:
 *  1. plan create wrote `.octocode/plan/**` scaffolding at the git root even
 *     when the caller passed an explicit subdirectory workspace.
 *  2. workspace-status / repo-profile lock counts dropped NULL-expiry
 *     (permanent) locks that the conflict checker still honors.
 *  3. domain-validation CLI errors were bare {error} while flag-parse errors
 *     carried {command,schema,example} recovery context.
 *  5. lock release did not accept the lock_id that lock acquire returns.
 */

import { describe, it, expect } from 'vitest';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { initDb } from '../src/db.js';
import { canonicalizePath } from '../src/git.js';
import { createPlan } from '../src/plans.js';
import { preFlightIntent } from '../src/intents.js';
import { getWorkspaceStatus } from '../src/maintenance.js';
import { queryAwareness } from '../src/repo-context.js';

const SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../out/octocode-awareness.js');
const NODE = process.execPath;

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function mktemp(prefix: string): string {
  return mkdtempSync(join(tmpdir(), prefix));
}

function initGitRepo(root: string): string {
  const result = spawnSync('git', ['init', '-q'], { cwd: root, encoding: 'utf8', timeout: 5000 });
  expect(result.status, result.stderr || result.stdout).toBe(0);
  const gitRoot = spawnSync('git', ['-C', root, 'rev-parse', '--show-toplevel'], { encoding: 'utf8', timeout: 5000 });
  expect(gitRoot.status, gitRoot.stderr || gitRoot.stdout).toBe(0);
  return gitRoot.stdout.trim();
}

function run(dbPath: string, args: string[], opts: { cwd?: string } = {}): {
  status: number; stdout: string; parsed: Record<string, unknown> | null;
} {
  const result = spawnSync(NODE, [SCRIPT, '--db', dbPath, ...args], {
    cwd: opts.cwd ?? process.cwd(),
    encoding: 'utf8',
    timeout: 30000,
  });
  let parsed: Record<string, unknown> | null = null;
  try { parsed = JSON.parse(result.stdout) as Record<string, unknown>; } catch { /* non-JSON */ }
  return { status: result.status ?? 1, stdout: result.stdout, parsed };
}

describe('plan create doc placement (finding 1)', () => {
  it('writes scaffolding under an explicit subdir workspace while scoping the plan to the git root', () => {
    const tmp = mktemp('oc-plan-scope-');
    try {
      const gitRoot = initGitRepo(tmp);
      const subdir = join(tmp, 'scratch', 'isolated');
      mkdirSync(subdir, { recursive: true });

      const db = freshDb();
      const created = createPlan(db, {
        name: 'Isolated scaffold',
        objective: 'Docs must land where the caller asked.',
        leadAgentId: 'lead',
        workspacePath: subdir,
        docsPath: subdir,
      });

      expect(created.plan.workspace_path).toBe(canonicalizePath(gitRoot));
      expect(created.plan.doc_dir).toMatch(/^scratch\/isolated\/\.octocode\/plan\//);
      expect(existsSync(join(subdir, '.octocode', 'plan'))).toBe(true);
      expect(existsSync(join(gitRoot, '.octocode'))).toBe(false);
      expect(existsSync(created.document_path)).toBe(true);
      // The stored (workspace_path, doc_dir) pair still resolves to the docs.
      expect(resolve(created.plan.workspace_path, created.plan.doc_dir, 'PLAN.md'))
        .toBe(created.document_path);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('keeps root placement when no docsPath is given', () => {
    const tmp = mktemp('oc-plan-root-');
    try {
      const gitRoot = initGitRepo(tmp);
      const subdir = join(tmp, 'packages', 'thing');
      mkdirSync(subdir, { recursive: true });

      const db = freshDb();
      const created = createPlan(db, {
        name: 'Root scaffold',
        objective: 'Default stays at the shared root.',
        leadAgentId: 'lead',
        workspacePath: subdir,
      });

      expect(created.plan.doc_dir).toMatch(/^\.octocode\/plan\//);
      expect(existsSync(join(gitRoot, '.octocode', 'plan'))).toBe(true);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('rejects a docsPath outside the workspace root', () => {
    const inside = mktemp('oc-plan-in-');
    const outside = mktemp('oc-plan-out-');
    try {
      initGitRepo(inside);
      const db = freshDb();
      expect(() => createPlan(db, {
        name: 'Escape attempt',
        objective: 'Docs must not escape the workspace.',
        leadAgentId: 'lead',
        workspacePath: inside,
        docsPath: outside,
      })).toThrow(/must be inside the workspace root/);
    } finally {
      rmSync(inside, { recursive: true, force: true });
      rmSync(outside, { recursive: true, force: true });
    }
  });

  it('honors an explicit --workspace subdir via the CLI', () => {
    const tmp = mktemp('oc-plan-cli-');
    try {
      const gitRoot = initGitRepo(tmp);
      const subdir = join(tmp, 'scratch');
      mkdirSync(subdir, { recursive: true });
      const dbPath = join(tmp, 'scratch.sqlite3');

      const r = run(dbPath, [
        'plan', 'create',
        '--name', 'CLI isolated plan',
        '--objective', 'Scaffolding follows --workspace.',
        '--lead-agent-id', 'lead',
        '--workspace', subdir,
        '--compact',
      ]);
      expect(r.status, r.stdout).toBe(0);
      expect(existsSync(join(subdir, '.octocode', 'plan'))).toBe(true);
      expect(existsSync(join(gitRoot, '.octocode'))).toBe(false);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});

describe('NULL-expiry locks in dashboards (finding 2)', () => {
  function withPermanentLock(): { db: DatabaseSync; workspace: string } {
    const workspace = mktemp('oc-lock-null-');
    const db = freshDb();
    const result = preFlightIntent(db, {
      agentId: 'legacy-agent',
      workspacePath: workspace,
      targetFiles: [join(workspace, 'src/a.ts')],
      rationale: 'legacy permanent lock',
    });
    expect(result.ok).toBe(true);
    // Legacy permanent locks have no expiry; the conflict checker treats
    // them as active forever, so dashboards must count them too.
    db.prepare('UPDATE locks SET expires_at = NULL').run();
    return { db, workspace };
  }

  it('counts them in workspace status', () => {
    const { db, workspace } = withPermanentLock();
    try {
      const status = getWorkspaceStatus(db, { workspace_path: workspace });
      expect(status.lock_count).toBe(1);
      expect(status.locks.length).toBe(1);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('counts them in the repo profile active_locks metric', () => {
    const { db, workspace } = withPermanentLock();
    try {
      const result = queryAwareness(db, { view: 'repo-profile', workspace });
      const metric = result.rows.find((row) => row['metric'] === 'active_locks');
      expect(metric?.['count']).toBe(1);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});

describe('structured domain errors (finding 3)', () => {
  it('attaches command/schema/example to domain-validation errors', () => {
    const tmp = mktemp('oc-errshape-');
    try {
      const dbPath = join(tmp, 'db.sqlite3');
      const r = run(dbPath, [
        'memory', 'record',
        '--agent-id', 'agent',
        '--task-context', 'ctx',
        '--observation', 'obs',
        '--importance', '99',
        '--compact',
      ]);
      expect(r.status).toBe(1);
      expect(r.parsed?.['ok']).toBe(false);
      expect(r.parsed?.['error']).toContain('--importance');
      expect(r.parsed?.['command']).toBe('memory record');
      expect(String(r.parsed?.['schema'])).toContain('schema json-schema memory_record');
      expect(String(r.parsed?.['example'])).toContain('memory record');
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('attaches context to errors thrown from domain modules', () => {
    const tmp = mktemp('oc-errshape2-');
    try {
      const dbPath = join(tmp, 'db.sqlite3');
      // Unknown label hard-errors inside src/memory.ts, past flag parsing.
      const r = run(dbPath, [
        'memory', 'record',
        '--agent-id', 'agent',
        '--task-context', 'ctx',
        '--observation', 'obs',
        '--importance', '5',
        '--label', 'NOT_A_LABEL',
        '--compact',
      ]);
      expect(r.status).toBe(1);
      expect(r.parsed?.['ok']).toBe(false);
      expect(r.parsed?.['command']).toBe('memory record');
      expect(String(r.parsed?.['schema'])).toContain('memory_record');
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});

describe('lock release --lock-id (finding 5)', () => {
  it('releases via the lock_id returned by lock acquire', () => {
    const tmp = mktemp('oc-lockid-');
    try {
      const dbPath = join(tmp, 'db.sqlite3');
      const target = join(tmp, 'src/a.ts');
      const acquired = run(dbPath, [
        'lock', 'acquire',
        '--agent-id', 'agent',
        '--workspace', tmp,
        '--target-file', target,
        '--rationale', 'edit',
        '--compact',
      ]);
      expect(acquired.status, acquired.stdout).toBe(0);
      const runInfo = acquired.parsed?.['run'] as Record<string, unknown>;
      const locks = runInfo['locks'] as Array<Record<string, unknown>>;
      const lockId = String(locks[0]?.['lock_id']);
      expect(lockId).toBeTruthy();

      const released = run(dbPath, [
        'lock', 'release',
        '--agent-id', 'agent',
        '--lock-id', lockId,
        '--status', 'PENDING',
        '--compact',
      ]);
      // Exit 2 = released but verification pending; both mean the release ran.
      expect([0, 2], released.stdout).toContain(released.status);
      expect(released.parsed?.['released']).toBeTruthy();
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('rejects an unknown lock id with a clear error', () => {
    const tmp = mktemp('oc-lockid2-');
    try {
      const dbPath = join(tmp, 'db.sqlite3');
      const r = run(dbPath, [
        'lock', 'release',
        '--agent-id', 'agent',
        '--lock-id', 'lok_does_not_exist',
        '--compact',
      ]);
      expect(r.status).toBe(1);
      expect(String(r.parsed?.['error'])).toContain('lock not found');
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});
