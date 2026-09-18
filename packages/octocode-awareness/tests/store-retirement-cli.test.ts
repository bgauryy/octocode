import { existsSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { z } from 'zod';
import { executeAwarenessCli } from '../src/command-cli.js';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach(root => rmSync(root, { recursive: true, force: true })));

function fixture() {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-store-cli-')));
  roots.push(workspace);
  return { workspace, database: join(workspace, 'awareness.sqlite3') };
}

describe('operator store retirement CLI', () => {
  it('publishes an executable canonical schema under the operator budget', async () => {
    const result = await executeAwarenessCli(['schema', 'command', 'maintenance', 'store-retire', '--compact']);
    expect(result.exitCode, JSON.stringify(result)).toBe(0);
    expect(result.payload).toMatchObject({
      'x-awareness-operation': 'maintenance.store-retire',
      'x-cli-command': 'maintenance store-retire',
      properties: { action: { enum: ['report', 'apply'], default: 'report' } },
    });
    expect(Buffer.byteLength(JSON.stringify(result.payload))).toBeLessThanOrEqual(2_000);
    const executable = z.fromJSONSchema(result.payload as Record<string, unknown>);
    expect(executable.safeParse({}).success).toBe(true);
    expect(executable.safeParse({ action: 'apply', confirm: 'retire', report_file: '/tmp/report.json' }).success).toBe(true);
    expect(executable.safeParse({ action: 'apply' }).success).toBe(false);
    expect(executable.safeParse({ action: 'report', confirm: 'retire' }).success).toBe(false);
  });

  it('reports by default and rejects apply without exact confirmation', async () => {
    const { workspace, database } = fixture();
    const initialized = await executeAwarenessCli([
      '--db', database, 'maintenance', 'retention', '--workspace', workspace, '--compact',
    ]);
    expect(initialized.exitCode).toBe(0);

    const report = await executeAwarenessCli([
      '--db', database, 'maintenance', 'store-retire', '--workspace', workspace, '--compact',
    ]);
    expect(report).toMatchObject({ exitCode: 0, payload: { action: 'report', dry_run: true, can_apply: true } });
    expect(existsSync(database)).toBe(true);

    const rejected = await executeAwarenessCli([
      '--db', database, 'maintenance', 'store-retire', '--workspace', workspace, '--action', 'apply', '--compact',
    ]);
    expect(rejected.exitCode).toBe(1);
    expect(existsSync(database)).toBe(true);
  });

  it('quarantines only a confirmed temporary store and returns recovery paths', async () => {
    const { workspace, database } = fixture();
    expect((await executeAwarenessCli([
      '--db', database, 'maintenance', 'retention', '--workspace', workspace, '--compact',
    ])).exitCode).toBe(0);

    const preview = await executeAwarenessCli([
      '--db', database, 'maintenance', 'store-retire', '--workspace', workspace, '--compact',
    ]);
    expect(preview.exitCode).toBe(0);
    const reportFile = join(workspace, 'retirement-report.json');
    writeFileSync(reportFile, JSON.stringify(preview.payload));

    const result = await executeAwarenessCli([
      '--db', database, 'maintenance', 'store-retire', '--workspace', workspace,
      '--action', 'apply', '--confirm', 'retire', '--report-file', reportFile, '--compact',
    ]);
    expect(result).toMatchObject({
      exitCode: 0,
      payload: { action: 'apply', status: 'quarantined', recovery: expect.stringContaining('rename') },
    });
    expect(existsSync(database)).toBe(false);
    const moved = (result.payload as { quarantined: Array<{ source: string; quarantine: string }> }).quarantined;
    const databaseTarget = moved.find(target => target.source === database);
    expect(databaseTarget).toBeDefined();
    expect(existsSync(databaseTarget!.quarantine)).toBe(true);
  });

  it('derives help without adding retirement to routine agent discovery', async () => {
    const help = await executeAwarenessCli(['maintenance', 'store-retire', '--help']);
    expect(help).toMatchObject({ exitCode: 0, text: expect.stringContaining('maintenance store-retire') });
    expect(help.text).toContain('--confirm retire');
    const routine = await executeAwarenessCli(['schema', 'commands', '--compact']);
    expect((routine.payload as { operations: string[] }).operations).not.toContain('maintenance.store-retire');
  });
});
