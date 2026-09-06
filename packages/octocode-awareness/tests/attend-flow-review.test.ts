import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { initDb } from '../src/db-init.js';
import { attendAwareness } from '../src/attend-query.js';
import { tsxCli } from './helpers/tsx-cli.js';

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SOURCE_SCRIPT = resolve(PACKAGE_ROOT, 'bin/awareness.ts');
const TSX_SCRIPT = tsxCli;
const cleanups: Array<() => void> = [];

afterEach(() => cleanups.splice(0).forEach(cleanup => cleanup()));

function fixture() {
  const workspace = mkdtempSync(join(realpathSync(tmpdir()), 'attend-flow-review-'));
  const dbPath = join(workspace, 'literal args store.sqlite3');
  const db = new DatabaseSync(dbPath);
  initDb(db);
  cleanups.push(() => { db.close(); rmSync(workspace, { recursive: true, force: true }); });
  return { db, dbPath, workspace };
}

function activePlan(db: DatabaseSync, workspace: string): void {
  const now = new Date().toISOString();
  db.prepare(`INSERT INTO awareness_plans
    (plan_id, name, objective, lead_agent_id, status, workspace_path, doc_dir, created_at, updated_at)
    VALUES ('plan_review', 'review', 'review flow', 'lead', 'ACTIVE', ?, '.', ?, ?)`).run(workspace, now, now);
}

describe('attend structured-next adversarial review', () => {
  it('uses a literal read-only task-show route for ready work when identity is unknown', () => {
    const { db, dbPath, workspace } = fixture();
    const now = new Date().toISOString();
    activePlan(db, workspace);
    db.prepare(`INSERT INTO awareness_tasks
      (task_id, plan_id, title, reasoning, acceptance_criteria, status, priority, created_by, created_at, updated_at)
      VALUES ('task_ready', 'plan_review', 'ready', 'inspect first', 'checked', 'OPEN', 1, 'lead', ?, ?)`).run(now, now);
    const before = db.prepare("SELECT status, updated_at FROM awareness_tasks WHERE task_id = 'task_ready'").get();

    const packet = attendAwareness(db, { workspacePath: workspace, compact: true });

    expect(packet.next).toMatchObject({
      action: 'inspect_ready_task', target: { task_id: 'task_ready' },
      command: {
        name: 'task show',
        args: ['--db', dbPath, '--workspace', workspace, '--task-id', 'task_ready', '--compact'],
      },
    });
    const executable = spawnSync(process.execPath, [TSX_SCRIPT, SOURCE_SCRIPT, 'task', 'show', ...packet.next.command!.args], {
      cwd: workspace, encoding: 'utf8', timeout: 30_000,
    });
    expect(executable.status, executable.stderr || executable.stdout).toBe(0);
    expect(JSON.parse(executable.stdout)).toMatchObject({ ok: true });
    expect(db.prepare("SELECT status, updated_at FROM awareness_tasks WHERE task_id = 'task_ready'").get()).toEqual(before);
  });

  it('inspects scoped peer work without claiming it when caller identity is unknown', () => {
    const { db, dbPath, workspace } = fixture();
    const now = new Date().toISOString();
    const file = join(workspace, 'src', 'shared.ts');
    const future = new Date(Date.now() + 60_000).toISOString();
    db.prepare(`INSERT INTO task_runs
      (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
      VALUES ('run_peer', 'WORK', 'peer', 'bounded peer work', 'focused check', 'ACTIVE', ?, ?, ?)`).run(workspace, now, now);
    db.prepare(`INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
      VALUES ('run_peer', ?, 'EXPLICIT', ?, ?, ?)`).run(file, now, now, future);
    const before = db.prepare("SELECT status, agent_id, updated_at FROM task_runs WHERE run_id = 'run_peer'").get();

    const packet = attendAwareness(db, { workspacePath: workspace, file: 'src/shared.ts', compact: true });

    expect(packet.next).toMatchObject({
      action: 'inspect_overlap',
      target: { file: 'src/shared.ts' },
      command: {
        name: 'work show',
        args: ['--db', dbPath, '--workspace', workspace, '--file', 'src/shared.ts', '--compact'],
      },
    });
    expect(packet.next).not.toHaveProperty('claim');
    expect(packet.next).not.toHaveProperty('heartbeat');

    const executable = spawnSync(process.execPath, [TSX_SCRIPT, SOURCE_SCRIPT, 'work', 'show', ...packet.next.command!.args], {
      cwd: workspace,
      encoding: 'utf8',
      timeout: 30_000,
    });
    expect(executable.status, executable.stderr || executable.stdout).toBe(0);
    expect(JSON.parse(executable.stdout)).toMatchObject({ ok: true });
    expect(db.prepare("SELECT status, agent_id, updated_at FROM task_runs WHERE run_id = 'run_peer'").get()).toEqual(before);
  });

  it('resumes owned work without emitting a heartbeat command or changing its lease', () => {
    const { db, workspace } = fixture();
    const now = new Date().toISOString();
    const future = new Date(Date.now() + 60_000).toISOString();
    activePlan(db, workspace);
    db.prepare(`INSERT INTO awareness_tasks
      (task_id, plan_id, title, reasoning, acceptance_criteria, status, priority, created_by, created_at, updated_at)
      VALUES ('task_owned', 'plan_review', 'owned', 'continue current work', 'checked', 'IN_PROGRESS', 1, 'owner', ?, ?)`).run(now, now);
    db.prepare(`INSERT INTO task_runs
      (run_id, task_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
      VALUES ('run_owned', 'task_owned', 'TASK', 'owner', 'bounded work', 'focused check', 'ACTIVE', ?, ?, ?)`).run(workspace, now, now);
    db.prepare(`INSERT INTO task_claims (task_id, run_id, agent_id, claimed_at, heartbeat_at, expires_at)
      VALUES ('task_owned', 'run_owned', 'owner', ?, ?, ?)`).run(now, now, future);
    const before = db.prepare("SELECT heartbeat_at, expires_at FROM task_claims WHERE task_id = 'task_owned'").get();

    const packet = attendAwareness(db, { workspacePath: workspace, agentId: 'owner', compact: true });

    expect(packet.next).toMatchObject({ action: 'resume_owned_task', target: { task_id: 'task_owned', run_id: 'run_owned' } });
    expect(packet.next).not.toHaveProperty('command');
    expect(db.prepare("SELECT heartbeat_at, expires_at FROM task_claims WHERE task_id = 'task_owned'").get()).toEqual(before);
  });
});
