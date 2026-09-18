import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, realpathSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { executeAwarenessCli } from '../src/command-cli.js';
import { ROUTINE_AWARENESS_OPERATIONS } from '../src/schema/operation-catalog.js';
import { connectDb } from '../src/db-runtime.js';
import { runMaintenanceRetention } from '../src/maintenance-retention.js';
import { maintenanceRetentionSchema } from '../src/schema/definitions-maintenance.js';
import { z } from 'zod';

const roots: string[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function fixture(): { workspace: string; database: string } {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-retention-cli-')));
  roots.push(workspace);
  return { workspace, database: join(workspace, 'awareness.sqlite3') };
}

describe('operator maintenance retention contract', () => {
  it('discovers one bounded canonical operator schema without changing routine discovery', async () => {
    const schema = await executeAwarenessCli(['schema', 'command', 'maintenance', 'retention', '--compact']);
    expect(schema.exitCode, JSON.stringify(schema.payload)).toBe(0);
    expect(schema.payload).toMatchObject({
      'x-awareness-operation': 'maintenance.retention',
      'x-cli-command': 'maintenance retention',
      properties: {
        action: { enum: ['report', 'apply'], default: 'report' },
        confirm: expect.any(Object),
      },
    });
    expect(Buffer.byteLength(JSON.stringify(schema.payload))).toBeLessThanOrEqual(2_000);
    const executable = z.fromJSONSchema(schema.payload as Record<string, unknown>);
    expect(executable.safeParse({}).success).toBe(true);
    expect(executable.safeParse({ action: 'apply', confirm: 'apply-retention' }).success).toBe(true);
    expect(executable.safeParse({ action: 'apply' }).success).toBe(false);
    expect(executable.safeParse({ action: 'report', confirm: 'apply-retention' }).success).toBe(false);

    const routine = await executeAwarenessCli(['schema', 'commands', '--compact']);
    expect((routine.payload as { operations: string[] }).operations).toEqual(ROUTINE_AWARENESS_OPERATIONS);
    expect((routine.payload as { operations: string[] }).operations).not.toContain('maintenance.retention');
  });

  it('is a dry run by default and rejects mutation without the exact confirmation', async () => {
    const { workspace, database } = fixture();
    const report = await executeAwarenessCli([
      '--db', database, 'maintenance', 'retention', '--workspace', workspace, '--compact',
    ]);
    expect(report.exitCode, JSON.stringify(report)).toBe(0);
    expect(report).toMatchObject({
      exitCode: 0,
      payload: { ok: true, operation: 'maintenance.retention', action: 'report', dry_run: true },
    });

    for (const args of [
      ['--action', 'apply'],
      ['--action', 'apply', '--confirm', 'apply'],
      ['--action', 'report', '--confirm', 'apply-retention'],
    ]) {
      const result = await executeAwarenessCli([
        '--db', database, 'maintenance', 'retention', '--workspace', workspace, ...args, '--compact',
      ]);
      expect(result.exitCode, JSON.stringify({ args, result })).toBe(1);
    }
  });

  it('derives operator help from the canonical schema', async () => {
    const help = await executeAwarenessCli(['maintenance', 'retention', '--help']);
    expect(help).toMatchObject({ exitCode: 0, text: expect.stringContaining('maintenance retention') });
    expect(help.text).toContain('--action');
    expect(help.text).toContain('--confirm');
    expect(help.text).toContain('apply-retention');
  });

  it('normalizes a symlinked checkout path to the canonical workspace scope', async () => {
    const { workspace, database } = fixture();
    const alias = join(workspace, 'alias');
    symlinkSync(workspace, alias, 'dir');
    expect((await executeAwarenessCli([
      '--db', database, 'maintenance', 'retention', '--workspace', workspace, '--compact',
    ])).exitCode).toBe(0);
    const db = connectDb(database);
    const old = '2020-01-01T00:00:00.000Z';
    db.prepare(`INSERT INTO task_runs(
      run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at
    ) VALUES ('nested-scope-run', 'WORK', 'agent', 'done', 'test', 'SUCCESS', ?, ?, ?)`)
      .run(workspace, old, old);
    db.close();

    const report = await executeAwarenessCli([
      '--db', database, 'maintenance', 'retention', '--workspace', alias, '--compact',
    ]);
    expect((report.payload as { workspace: string }).workspace).toBe(workspace);
    expect(report).toMatchObject({ exitCode: 0, payload: { domains: { runs: { matched: 1 } } } });
  });

  it('applies owner results atomically and reports committed counts', async () => {
    const { workspace, database } = fixture();
    expect((await executeAwarenessCli([
      '--db', database, 'maintenance', 'retention', '--workspace', workspace, '--compact',
    ])).exitCode).toBe(0);
    let db = connectDb(database);
    const old = '2020-01-01T00:00:00.000Z';
    db.prepare(`INSERT INTO pending_interactions(
      interaction_id, workspace_path, session_id, correlation_id, kind, request_json,
      status, created_at, expires_at
    ) VALUES ('elapsed', ?, 'session', 'correlation', 'question', '{}', 'pending', ?, ?)`)
      .run(workspace, old, old);
    db.prepare(`INSERT INTO task_runs(
      run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at
    ) VALUES ('old-run', 'WORK', 'agent', 'done', 'test', 'SUCCESS', ?, ?, ?)`)
      .run(workspace, old, old);
    db.exec(`CREATE TRIGGER fail_run_retention BEFORE DELETE ON task_runs
      WHEN OLD.run_id = 'old-run' BEGIN SELECT RAISE(ABORT, 'forced retention failure'); END`);
    expect(() => runMaintenanceRetention(db, workspace, maintenanceRetentionSchema.parse({
      action: 'apply', confirm: 'apply-retention',
    }))).toThrow(/forced retention failure/);
    expect(db.prepare("SELECT status FROM pending_interactions WHERE interaction_id = 'elapsed'").get())
      .toEqual({ status: 'pending' });
    expect(db.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE run_id = 'old-run'").get())
      .toEqual({ count: 1 });
    db.exec('DROP TRIGGER fail_run_retention');
    db.close();

    const applied = await executeAwarenessCli([
      '--db', database, 'maintenance', 'retention', '--workspace', workspace,
      '--action', 'apply', '--confirm', 'apply-retention', '--compact',
    ]);
    expect(applied).toMatchObject({
      exitCode: 0,
      payload: {
        ok: true,
        operation: 'maintenance.retention',
        action: 'apply',
        dry_run: false,
        domains: {
          interactions: { expired: 1, matched: 1, deleted: 1 },
          runs: { matched: 1, deleted: 1 },
        },
        totals: { matched: 2, deleted: 2, expired: 1 },
      },
    });
  });

  it('executes bounded continuations to quiescence without losing eligible rows', async () => {
    const { workspace, database } = fixture();
    expect((await executeAwarenessCli([
      '--db', database, 'maintenance', 'retention', '--workspace', workspace, '--compact',
    ])).exitCode).toBe(0);
    const db = connectDb(database);
    const old = '2020-01-01T00:00:00.000Z';
    const insert = db.prepare(`INSERT INTO task_runs(
      run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at
    ) VALUES (?, 'WORK', 'agent', 'done', 'test', 'SUCCESS', ?, ?, ?)`);
    for (const id of ['run-a', 'run-b', 'run-c']) insert.run(id, workspace, old, old);
    db.close();

    let argv = [
      'maintenance', 'retention', '--db', database, '--workspace', workspace,
      '--action', 'apply', '--confirm', 'apply-retention', '--limit', '1', '--compact',
    ];
    let deleted = 0;
    let calls = 0;
    let asOf: string | undefined;
    for (;;) {
      const result = await executeAwarenessCli(argv);
      expect(result.exitCode, JSON.stringify(result)).toBe(0);
      const payload = result.payload as {
        as_of: string;
        domains: { runs: { deleted: number } };
        partial: boolean;
        next: null | { args: { as_of: string }; argv: string[] };
      };
      asOf ??= payload.as_of;
      expect(payload.as_of).toBe(asOf);
      deleted += payload.domains.runs.deleted;
      calls++;
      if (!payload.partial) {
        expect(payload.next).toBeNull();
        break;
      }
      expect(payload.next?.args.as_of).toBe(asOf);
      argv = payload.next!.argv;
      expect(calls).toBeLessThan(10);
    }
    expect({ calls, deleted }).toEqual({ calls: 3, deleted: 3 });
    const check = connectDb(database);
    expect(check.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE run_id LIKE 'run-%'").get())
      .toEqual({ count: 0 });
    check.close();
  });
});
