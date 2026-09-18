import { afterEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { executeAwarenessCli } from '../src/command-cli.js';
import { parseArgs } from '../src/command-parser.js';

const roots: string[] = [];

afterEach(() => {
  vi.unstubAllEnvs();
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function fixture() {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-cli-boundaries-')));
  roots.push(workspace);
  return { workspace, database: join(workspace, 'awareness.sqlite3') };
}

describe('canonical CLI parsing and help boundaries', () => {
  it('tokenizes repeated, inline, negative, boolean, and positional-boundary arguments', () => {
    expect(parseArgs([
      'message', 'list', '--tag=first', '--tag', 'second', '--no-smart',
      '--compact=yes', '--limit=2', '--', 'literal', '--not-a-flag',
    ])).toEqual({
      _: ['message', 'list', 'literal', '--not-a-flag'],
      tag: ['first', 'second'],
      smart: false,
      compact: true,
      limit: '2',
    });
    expect(parseArgs(['--query=', '--include-bodies=0', '--file', 'a.ts', '--file=b.ts']))
      .toEqual({ _: [], query: true, include_bodies: false, file: ['a.ts', 'b.ts'] });
    expect(parseArgs(['--compact', '--full', 'no', '--title', 'value']))
      .toEqual({ _: [], compact: true, full: false, title: 'value' });
  });

  it('renders root, schema, concept, exact-operation, and fallback help', async () => {
    const root = await executeAwarenessCli([]);
    expect(root).toMatchObject({ exitCode: 0, payload: null, text: expect.any(String) });
    expect(root.text).toMatch(/context\s+orient/);
    expect((await executeAwarenessCli(['-h'])).text).toBe(root.text);
    const schema = await executeAwarenessCli(['schema', '--help']);
    expect(schema).toMatchObject({ exitCode: 0, text: expect.stringContaining('schema commands|command|entities') });
    const concept = await executeAwarenessCli(['work', '--help']);
    expect(concept).toMatchObject({ exitCode: 0, text: expect.stringContaining('work create|list|show|claim|update|depend|protect|verify') });
    const exact = await executeAwarenessCli(['memory', 'recall', '--help']);
    expect(exact).toMatchObject({ exitCode: 0, text: expect.stringContaining('memory recall') });
    expect(exact.text).toContain('--check-fingerprint');
    const fallback = await executeAwarenessCli(['retired', '--help', '--compact']);
    expect(fallback).toMatchObject({ exitCode: 0, text: expect.stringContaining('one direct surface') });
  });

  it('rejects malformed globals, positions, values, flags, and missing mutation identity', async () => {
    vi.stubEnv('OCTOCODE_AGENT_ID', undefined);
    const cases: Array<[string[], RegExp]> = [
      [['--db'], /--db expects a path/],
      [['--db-scope', '--compact', 'work', 'list'], /--db-scope expects a value/],
      [['schema', 'commands', 'extra'], /unexpected positional arguments/],
      [['schema', 'commands', '--future'], /Unknown flag --future/],
      [['message', 'send', '--kind', 'fyi', '--subject', 'Missing identity'], /require --agent-id/],
      [['message', 'list', '--include-bodies=maybe'], /expects a boolean/],
      [['message', 'list', '--limit=abc'], /expects an integer/],
      [['message', 'list', '--limit'], /expects an integer/],
      [['memory', 'recall', '--query', '--smart'], /--query expects a value/],
      [['memory', 'recall', '--no-query'], /--no-query is invalid/],
      [['message', 'list', '--unknown', 'value'], /Unknown flag --unknown/],
      [['--', 'message', 'list', 'extra'], /Unknown Awareness operation/],
    ];
    for (const [argv, error] of cases) {
      const result = await executeAwarenessCli(argv);
      expect(result.exitCode, JSON.stringify({ argv, result })).toBe(1);
      expect(result.payload).toMatchObject({ ok: false, error: expect.stringMatching(error) });
    }
  });

  it('coerces canonical scalar and collection fields with inline global bindings', async () => {
    const { workspace, database } = fixture();
    const listed = await executeAwarenessCli([
      `--db=${database}`, '--db-scope=repo', 'work', 'list', `--workspace=${workspace}`,
      '--kind=presence', '--limit=1', '--all=yes', '--no-full', '--compact=1',
    ]);
    expect(listed.exitCode, JSON.stringify(listed.payload)).toBe(0);
    expect(listed.payload).toMatchObject({ count: 0, total_count: 0 });

    const sent = await executeAwarenessCli([
      `--db=${database}`, 'message', 'send', `--workspace=${workspace}`, '--agent-id=sender',
      '--kind=fyi', '--subject=Inline collections', '--to-agent=reader', '--to-agent=reviewer',
      '--file=src/a.ts', '--file', 'src/b.ts', '--importance=6', '--compact=true',
    ]);
    expect(sent.exitCode, JSON.stringify(sent.payload)).toBe(0);
    expect(sent.payload).toMatchObject({ signal_id: expect.any(String) });
  });
});
