import { existsSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, describe, expect, it } from 'vitest';
import { executeAwarenessCli } from '../src/command-cli.js';
import { awarenessEntityCatalog } from '../src/schema/entities.js';

const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function fixture() {
  const workspace = mkdtempSync(join(tmpdir(), 'oc-awareness-view-'));
  roots.push(workspace);
  return {
    workspace,
    database: join(workspace, 'awareness.sqlite3'),
    output: join(workspace, 'status', 'awareness.html'),
  };
}

function signalCount(database: string): number {
  const db = new DatabaseSync(database, { readOnly: true });
  try {
    return Number((db.prepare('SELECT COUNT(*) AS count FROM signals').get() as { count: number }).count);
  } finally {
    db.close();
  }
}

describe('operator Awareness HTML view', () => {
  it('opens one private, self-contained page with every SQLite entity and LocalGit status', async () => {
    const { workspace, database, output } = fixture();
    const hostileSubject = '<script>alert("peer")</script>';
    const seeded = await executeAwarenessCli([
      '--db', database, 'message', 'send', '--workspace', workspace, '--agent-id', 'sender',
      '--kind', 'fyi', '--subject', hostileSubject, '--compact',
    ]);
    expect(seeded.exitCode, JSON.stringify(seeded.payload)).toBe(0);
    const before = signalCount(database);
    const databaseBefore = statSync(database);
    const opened: string[] = [];

    const result = await executeAwarenessCli([
      '--db', database, 'view', '--workspace', workspace, '--out', output, '--compact',
    ], { openFile: async path => { opened.push(path); } });

    expect(result).toMatchObject({
      exitCode: 0,
      payload: {
        ok: true,
        kind: 'awareness.view',
        path: output,
        opened: true,
        database,
        workspace: realpathSync(workspace),
      },
    });
    expect(opened).toEqual([output]);
    expect(signalCount(database)).toBe(before);
    const databaseAfter = statSync(database);
    expect(databaseAfter.mtimeMs).toBe(databaseBefore.mtimeMs);
    expect(databaseAfter.mode).toBe(databaseBefore.mode);
    expect(statSync(output).mode & 0o777).toBe(0o600);

    const html = readFileSync(output, 'utf8');
    expect(html).toContain('<!doctype html>');
    expect(html).toContain('LocalGit evidence');
    expect(html).toContain('isomorphic-git');
    expect(html).toContain('&lt;script&gt;alert(&quot;peer&quot;)&lt;/script&gt;');
    expect(html).not.toContain(hostileSubject);
    expect(html).not.toMatch(/https?:\/\//);
    for (const entity of awarenessEntityCatalog().entities) {
      expect(html, entity.name).toContain(`data-entity="${entity.name}"`);
    }
  });

  it('supports generation without opening and rejects unsupported flags', async () => {
    const { workspace, database, output } = fixture();
    const seeded = await executeAwarenessCli([
      '--db', database, 'message', 'send', '--workspace', workspace, '--agent-id', 'viewer',
      '--kind', 'fyi', '--subject', 'Initialize view fixture', '--compact',
    ]);
    expect(seeded.exitCode, JSON.stringify(seeded.payload)).toBe(0);
    let opened = false;
    const generated = await executeAwarenessCli([
      'view', '--db', database, '--workspace', workspace, '--out', output, '--no-open', '--compact',
    ], { openFile: async () => { opened = true; } });
    expect(generated).toMatchObject({ exitCode: 0, payload: { opened: false, path: output } });
    expect(opened).toBe(false);

    const openFailure = await executeAwarenessCli([
      'view', '--db', database, '--workspace', workspace, '--out', join(workspace, 'open-failed.html'), '--compact',
    ], { openFile: async () => { throw new Error('browser unavailable'); } });
    expect(openFailure).toMatchObject({
      exitCode: 0,
      payload: { opened: false, open_error: 'browser unavailable' },
      diagnostics: [expect.stringContaining('browser unavailable')],
    });

    const invalid = await executeAwarenessCli(['view', '--db', database, '--workspace', workspace, '--mystery']);
    expect(invalid).toMatchObject({ exitCode: 1, payload: { ok: false, error: 'Unknown flag --mystery' } });
  });

  it('refuses to create a missing database while inspecting', async () => {
    const { workspace, database, output } = fixture();
    expect(existsSync(database)).toBe(false);
    const result = await executeAwarenessCli([
      'view', '--db', database, '--workspace', workspace, '--out', output, '--no-open', '--compact',
    ]);
    expect(result).toMatchObject({
      exitCode: 1,
      payload: { ok: false, error: expect.stringContaining('does not exist') },
    });
    expect(existsSync(database)).toBe(false);
    expect(existsSync(output)).toBe(false);
  });

  it('documents view as an operator command without adding it to the routine operation catalog', async () => {
    const help = await executeAwarenessCli(['view', '--help']);
    expect(help).toMatchObject({ exitCode: 0, text: expect.stringContaining('view [options]') });
    expect(help.text).toContain('--no-open');
    const catalog = await executeAwarenessCli(['schema', 'commands', '--compact']);
    expect(catalog.exitCode).toBe(0);
    expect(JSON.stringify(catalog.payload)).not.toContain('"view"');
  });
});
