import { DatabaseSync } from 'node:sqlite';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { initDb } from '../src/db-init.js';
import { getRefinements, insertRefinement } from '../src/refinements.js';
import type { GetRefinementsParams } from '../src/types/identity-memory.js';
import { cmdRefineGet } from '../src/commands/memory.js';
import { parseArgs } from '../src/command-parser.js';
import { operationSchemas } from '../src/schema/definitions-operations.js';

let db: DatabaseSync;
let expected: string[];
const scope = { workspacePath: '/refinement-pages', repo: 'fixture/repo', ref: 'test', artifact: 'pages' };
beforeEach(() => {
  db = new DatabaseSync(':memory:');
  initDb(db);
  expected = Array.from({ length: 55 }, (_, i) => insertRefinement(db, {
    ...scope, agentId: 'owner', reasoning: 'fixture', remember: `match ${i}`, quality: 'bad', state: 'ongoing',
  }).refinementId);
  insertRefinement(db, { ...scope, reasoning: 'excluded', remember: 'wrong quality', quality: 'good' });
  insertRefinement(db, { ...scope, ref: 'other', reasoning: 'excluded', remember: 'wrong ref', quality: 'bad' });
});
afterEach(() => { vi.restoreAllMocks(); db.close(); });

describe('refinement pagination', () => {
  it.each([1, 50, 200])('executes API continuations covering 55 matches at limit %i', (limit) => {
    let params: GetRefinementsParams = { ...scope, states: ['ongoing'], quality: 'bad', limit };
    const seen: string[] = [];
    for (let page = 0; page < 60; page++) {
      const result = getRefinements(db, params);
      seen.push(...result.refinements.map(row => row.refinement_id));
      expect(result.partial).toBe(seen.length < expected.length);
      if (!result.partial) break;
      expect(result.partialReasons).toEqual(['limit']);
      expect(result.next?.list.method).toBe('getRefinements');
      params = result.next!.list.params;
      expect(operationSchemas.refine_query.safeParse({
        workspace_path: params.workspacePath, artifact: params.artifact, repo: params.repo, ref: params.ref,
        quality: params.quality, states: params.states, limit: params.limit, offset: params.offset,
      }).success).toBe(true);
    }
    expect(seen.length).toBe(new Set(seen).size);
    expect([...seen].sort()).toEqual([...expected].sort());
  });

  it.each([false, true].flatMap(full => [false, true].flatMap(compact => [1, 50, 200].map(limit => ({ full, compact, limit })))))('executes CLI command continuation with %j', ({ full, compact, limit }) => {
    const output = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    let argv = ['--workspace', scope.workspacePath, '--repo', scope.repo, '--ref', scope.ref,
      '--artifact', scope.artifact, '--quality', 'bad', '--state', 'ongoing', '--limit', String(limit), ...(compact ? ['--compact'] : []), ...(full ? ['--full'] : [])];
    const seen: string[] = [];
    for (let page = 0; page < 60; page++) {
      const args = parseArgs(argv);
      expect(cmdRefineGet(db, args, '/isolated/awareness.sqlite3', { compact })).toBe(0);
      const result = JSON.parse(String(output.mock.calls.at(-1)![0]));
      seen.push(...result.refinements.map((row: { refinement_id: string }) => row.refinement_id));
      if (full) expect(result.refinements[0]).toHaveProperty('remember');
      if (!result.partial) break;
      expect(result.next.list.command.name).toBe('refinement get');
      argv = result.next.list.command.args;
      expect(argv).toEqual(expect.arrayContaining(['--db', '/isolated/awareness.sqlite3']));
      expect(argv.includes('--compact')).toBe(compact);
      expect(argv.includes('--full')).toBe(full);
    }
    expect(seen.length).toBe(new Set(seen).size);
    expect([...seen].sort()).toEqual([...expected].sort());
  });

  it.each([-1, 0.5, Number.NaN, Number.MAX_SAFE_INTEGER + 1])('rejects invalid offset %s', (offset) => {
    expect(() => getRefinements(db, { ...scope, offset })).toThrow('offset must be a non-negative safe integer');
  });
});
