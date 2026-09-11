import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { connectDb } from '../src/db-runtime.js';
import { listWork, startWork } from '../src/work.js';
import type { ListWorkParams } from '../src/types/work-maintenance.js';
import { workSchemas } from '../src/schema/definitions-work.js';

let root: string;
let db: ReturnType<typeof connectDb>;
let dbPath: string;
let ids: string[];
let workspaceIds: string[];
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'aw-work-pages-'));
  dbPath = join(root, 'awareness.sqlite3');
  db = connectDb(dbPath);
  ids = [];
  for (let run = 0; run < 2; run++) {
    const result = startWork(db, {
      workspacePath: root, agentId: 'owner', rationale: `fixture ${run}`, testPlan: 'pagination union',
      targetFiles: Array.from({ length: 125 }, (_, index) => `file-${index}.ts`),
    });
    if (!result.ok) throw new Error('fixture failed');
    ids.push(...result.files.map(file => `${file.run_id}:${file.file_path}`));
  }
  workspaceIds = [...ids];
  const other = startWork(db, { workspacePath: root, agentId: 'other', rationale: 'other owner', testPlan: 'none', targetFiles: ['other.ts'] });
  if (!other.ok) throw new Error('other-owner fixture failed');
  workspaceIds.push(...other.files.map(file => `${file.run_id}:${file.file_path}`));
  startWork(db, { workspacePath: join(root, 'other-workspace'), agentId: 'owner', rationale: 'other workspace', testPlan: 'none', targetFiles: ['other.ts'] });
});
afterEach(() => { db.close(); rmSync(root, { recursive: true, force: true }); });

describe('lossless work listing', () => {
  it('executes API continuation parameters past 200 with filters preserved', () => {
    let params: ListWorkParams = { workspacePath: root, agentId: 'owner', limit: 200 };
    const seen: string[] = [];
    for (let page = 0; page < 3; page++) {
      expect(workSchemas.work.safeParse({ action: 'list', workspace: params.workspacePath, agent_id: params.agentId, limit: params.limit, offset: params.offset }).success).toBe(true);
      const result = listWork(db, params);
      seen.push(...result.files.map(file => `${file.run_id}:${file.file_path}`));
      if (!result.partial) break;
      expect(result.partialReasons).toEqual(['limit']);
      expect(result.next?.list.method).toBe('listWork');
      params = result.next!.list.params;
    }
    expect(new Set(seen).size).toBe(seen.length);
    expect([...seen].sort()).toEqual([...ids].sort());
  });

  it.each([false, true])('executes rebuilt CLI continuation with --full=%s and preserves default bounds', (full) => {
    const cli = resolve(import.meta.dirname, '../out/octocode-awareness.js');
    const base = ['work', 'list', '--db', dbPath, '--workspace', root, '--agent-id', 'owner', '--compact', ...(full ? ['--full'] : [])];
    const run = (args: string[]) => {
      const result = spawnSync(process.execPath, [cli, ...args], { encoding: 'utf8', timeout: 10000 });
      expect(result.status, result.stderr || result.stdout).toBe(0);
      return JSON.parse(result.stdout);
    };
    expect(run(base).count).toBe(5);
    expect(run(base.filter(arg => arg !== '--compact')).count).toBe(5);
    let args = [...base, '--limit', '200'];
    const seen: string[] = [];
    for (let page = 0; page < 3; page++) {
      const result = run(args);
      expect(result.total_count).toBe(workspaceIds.length);
      if (full) expect(result.files[0]).toHaveProperty('test_plan', 'pagination union');
      seen.push(...result.files.map((file: { run_id: string; file_path: string }) => `${file.run_id}:${file.file_path}`));
      if (!result.partial) break;
      expect(result.partialReasons).toEqual(['limit']);
      expect(result.next.list.command.name).toBe('work list');
      args = ['work', 'list', ...result.next.list.command.args];
    }
    expect(new Set(seen).size).toBe(seen.length);
    expect([...seen].sort()).toEqual([...workspaceIds].sort());
  });
});
