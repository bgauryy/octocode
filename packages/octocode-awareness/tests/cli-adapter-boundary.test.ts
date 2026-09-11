import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { executeAwarenessCli } from '../src/command-cli.js';
import { HELP, HELP_COMPACT } from '../src/cli-adapter/cli-help-data.js';
import { commandFromHelpArgv, helpFor, hyphenFlag } from '../src/cli-adapter/cli-help.js';
import { discoverBundledSkills } from '../src/cli-adapter/cli-model.js';
import { compactBriefItems, summarizeUtf8 } from '../src/maintenance-brief-format.js';
import {
  extractGlobalDb,
  normalizeToken,
  selectCommand,
  UNKNOWN_COMMAND,
  validateFlagValues,
} from '../src/cli-adapter/cli-routing.js';
import type { ParsedArgs } from '../src/commands/args.js';

const roots: string[] = [];

afterEach(() => {
  vi.unstubAllEnvs();
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function tempRoot(prefix = 'aw-cli-boundary-'): string {
  const root = mkdtempSync(join(tmpdir(), prefix));
  roots.push(root);
  return root;
}

function parsed(fields: Record<string, string | boolean | string[]>): ParsedArgs {
  return { _: [], ...fields };
}

describe('CLI adapter primitives', () => {
  it('bounds Unicode brief text by bytes and compacts duplicate coordination items', () => {
    const summary = summarizeUtf8('  alpha   🐙 beta  ', 14);
    expect(summary).toBe('alpha 🐙...');
    expect(Buffer.byteLength(summary, 'utf8')).toBeLessThanOrEqual(14);

    expect(compactBriefItems([
      { kind: 'memory', text: 'same', importance: 2 },
      { kind: 'memory', text: 'same', importance: 7 },
      { kind: 'notification', text: 'other' },
    ])).toEqual([
      { kind: 'memory', text: 'same (duplicate ×2)', importance: 7 },
      { kind: 'notification', text: 'other' },
    ]);
  });

  it('discovers only materialized skills and orders the required skill first', () => {
    expect(discoverBundledSkills(join(tempRoot(), 'missing'))).toEqual([]);
    const root = tempRoot('aw-cli-skills-');
    for (const name of ['zeta', 'octocode-awareness', 'alpha', 'empty']) {
      mkdirSync(join(root, name), { recursive: true });
      if (name !== 'empty') writeFileSync(join(root, name, 'SKILL.md'), `# ${name}\n`);
    }
    writeFileSync(join(root, 'plain-file'), 'ignored');

    expect(discoverBundledSkills(root)).toEqual([
      { name: 'octocode-awareness', path: join(root, 'octocode-awareness'), required: true },
      { name: 'alpha', path: join(root, 'alpha'), required: false },
      { name: 'zeta', path: join(root, 'zeta'), required: false },
    ]);
  });

  it('extracts global storage bindings in either form and rejects missing values', () => {
    expect(extractGlobalDb(['work', 'list', '--db', '/tmp/a.db', '--db-scope=repo', '--compact']))
      .toEqual({ dbPath: '/tmp/a.db', dbScope: 'repo', filtered: ['work', 'list', '--compact'] });
    expect(extractGlobalDb(['--db=/tmp/b.db', '--db-scope', 'global', 'status']))
      .toEqual({ dbPath: '/tmp/b.db', dbScope: 'global', filtered: ['status'] });
    expect(() => extractGlobalDb(['--db'])).toThrow('--db expects a path');
    expect(() => extractGlobalDb(['--db', '--compact'])).toThrow('--db expects a path');
    expect(() => extractGlobalDb(['--db-scope='])).toThrow('--db-scope expects a value');
  });

  it('validates booleans, integer flags, retention bounds and required values', () => {
    expect(() => validateFlagValues(parsed({ limit: '2', retention_days: '3650', compact: true }))).not.toThrow();
    expect(() => validateFlagValues(parsed({ agent_id: false }))).toThrow('--no-agent-id is invalid');
    expect(() => validateFlagValues(parsed({ compact: 'sometimes' }))).toThrow('--compact expects a boolean');
    expect(() => validateFlagValues(parsed({ limit: true }))).toThrow('--limit expects an integer');
    expect(() => validateFlagValues(parsed({ limit: '1.5' }))).toThrow('--limit expects an integer');
    expect(() => validateFlagValues(parsed({ retention_days: '0' }))).toThrow('--retention-days must be in 1..3650');
    expect(() => validateFlagValues(parsed({ query: true }))).toThrow('--query expects a value');
  });

  it('selects direct, hook, schema and legacy operator routes without aliases', () => {
    expect(normalizeToken('db_scope')).toBe('db-scope');
    expect(normalizeToken(undefined)).toBeUndefined();
    expect(selectCommand([])).toEqual({ command: undefined, rest: [] });
    expect(selectCommand(['--compact'])).toEqual({ command: undefined, rest: ['--compact'] });
    expect(selectCommand(['--compact', 'work', 'end', '--run-id', 'r1'])).toEqual({
      command: 'work-command', rest: ['--action', 'end', '--run-id', 'r1', '--compact'],
    });
    expect(selectCommand(['--wat', 'work', 'list'])).toEqual({ command: UNKNOWN_COMMAND, rest: ['--wat', 'work', 'list'] });
    expect(selectCommand(['hook', 'run', 'notify-deliver', '--strict'])).toEqual({ command: 'hook-run', rest: ['notify-deliver', '--strict'] });
    expect(selectCommand(['hook', 'run'])).toEqual({ command: 'hook-run', rest: [] });
    expect(selectCommand(['hooks', 'install', '--host', 'codex'])).toEqual({ command: 'hooks-install', rest: ['--host', 'codex'] });
    expect(selectCommand(['hooks', 'check', '--host', 'codex'])).toEqual({ command: 'hooks-install', rest: ['--check', '--host', 'codex'] });
    expect(selectCommand(['hooks', 'remove'])).toEqual({ command: 'hooks-install', rest: ['--remove'] });
    expect(selectCommand(['schema', 'commands', '--all'])).toEqual({ command: 'schema', rest: ['commands', '--all'] });
    expect(selectCommand(['query', 'workboard', '--limit', '1'])).toEqual({ command: 'query', rest: ['workboard', '--limit', '1'] });
    expect(selectCommand(['unknown', 'route'])).toEqual({ command: UNKNOWN_COMMAND, rest: ['unknown', 'route'] });
  });
});

describe('focused CLI help', () => {
  it('renders root, concept, operator and exact-operation help from one catalog', () => {
    expect(hyphenFlag('db_scope')).toBe('--db-scope');
    expect(helpFor(null)).toBe(HELP);
    expect(helpFor(null, { compact: true })).toBe(HELP_COMPACT);
    expect(helpFor(null, { routeKey: 'noun:schema' })).toContain('command <concept-or-operator>');
    expect(helpFor(null, { routeKey: 'noun:history' })).toContain('history status|timeline|read|restore');
    expect(helpFor(null, { routeKey: 'noun:hooks' })).toContain('hooks install|check|remove');
    expect(helpFor(null, { routeKey: 'noun:no-such-noun' })).toContain('no-such-noun [options]');

    const orient = helpFor('context.orient');
    expect(orient).toContain('context orient --workspace "$PWD" --compact');
    expect(orient).toContain('effect: read');
    expect(helpFor('message.send')).toContain('--to-agent');
    expect(helpFor('hooks-install', { routeKey: 'hooks check' })).toContain('hooks install|check|remove');
    expect(helpFor('hooks-install', { routeKey: 'hooks check', compact: true })).toContain('schema: hooks_check');
    expect(helpFor('definitely-unknown')).toBe(HELP);
  });

  it('derives help targets after removing help, compact and database bindings', () => {
    expect(commandFromHelpArgv(['--db', '/tmp/a.db', 'message', 'send', '--help', '--compact']))
      .toEqual({ command: 'message.send', routeKey: 'message send' });
    expect(commandFromHelpArgv(['hooks', 'check', '--help'])).toEqual({ command: 'hooks check', routeKey: 'hooks check' });
    expect(commandFromHelpArgv(['query', '--help'])).toEqual({ command: 'query', routeKey: 'query' });
    expect(commandFromHelpArgv(['memory', '--help'])).toEqual({ command: null, routeKey: 'noun:memory' });
    expect(commandFromHelpArgv(['hooks', '--help'])).toEqual({ command: null, routeKey: 'noun:hooks' });
    expect(commandFromHelpArgv(['mystery', '--help'])).toEqual({ command: UNKNOWN_COMMAND, routeKey: undefined });
  });
});

describe('canonical and operator CLI behavior', () => {
  it('keeps canonical identity and storage bindings at the host boundary', async () => {
    const root = tempRoot();
    const db = join(root, 'awareness.sqlite3');
    vi.stubEnv('OCTOCODE_AGENT_ID', undefined);
    const missingIdentity = await executeAwarenessCli([
      'message', 'send', '--db', db, '--workspace', root, '--kind', 'fyi', '--subject', 'missing',
    ]);
    expect(missingIdentity).toMatchObject({ exitCode: 1, payload: { error: expect.stringContaining('--agent-id') } });

    vi.stubEnv('OCTOCODE_AGENT_ID', 'env-sender');
    const sent = await executeAwarenessCli([
      '--compact', 'message', 'send', '--db', db, '--workspace', root,
      '--kind', 'fyi', '--subject', 'host-bound', '--to-agent', 'peer', '--session-id', 'session-1',
    ]);
    expect(sent.exitCode, JSON.stringify(sent.payload)).toBe(0);
    const listed = await executeAwarenessCli([
      'message', 'list', '--db', db, '--workspace', root, '--agent-id', 'peer', '--include-bodies', 'true', '--compact',
    ]);
    expect(listed.exitCode, JSON.stringify(listed.payload)).toBe(0);
    expect(JSON.stringify(listed.payload)).toContain('host-bound');
    expect((await executeAwarenessCli(['history', 'status', '--db-scope', 'invalid', '--workspace', root])).exitCode).toBe(1);
  });

  it('rejects malformed canonical flags and positionals before execution', async () => {
    expect(await executeAwarenessCli(['message', 'send', 'extra', '--agent-id', 'a']))
      .toMatchObject({ exitCode: 1, payload: { error: 'unexpected positional arguments' } });
    expect(await executeAwarenessCli(['memory', 'recall', '--labels', 'GOTCHA']))
      .toMatchObject({ exitCode: 1, payload: { error: expect.stringContaining('--labels') } });
    expect(await executeAwarenessCli(['memory', 'recall', '--limit', '1.5']))
      .toMatchObject({ exitCode: 1, payload: { error: expect.stringContaining('expects an integer') } });
  });

  it('projects operator discovery and exercises schema validation branches', async () => {
    const operator = await executeAwarenessCli(['schema', 'commands', '--all', '--examples']);
    expect(operator.exitCode).toBe(0);
    expect(operator.payload).toMatchObject({
      kind: 'awareness.cli-surface',
      operator: expect.arrayContaining([expect.objectContaining({ command: 'database consolidate', example: expect.any(String) })]),
    });
    expect((await executeAwarenessCli(['schema', 'command', 'missing', '--compact'])).payload)
      .toMatchObject({ error_code: 'UNKNOWN_CLI_COMMAND' });
    expect((await executeAwarenessCli(['schema', 'json-schema', 'missing', '--compact'])).payload)
      .toMatchObject({ error_code: 'UNKNOWN_SCHEMA' });
    expect((await executeAwarenessCli(['schema', 'validate', 'memory_recall', '--compact'])).payload)
      .toMatchObject({ issues: [expect.objectContaining({ path: 'input' })] });
    expect((await executeAwarenessCli(['schema', 'validate', 'memory_recall', '-'], { readStdin: async () => '{' })).payload)
      .toMatchObject({ error_code: 'INVALID_JSON' });
    expect((await executeAwarenessCli(['schema', 'validate', 'memory_recall', '-'], { readStdin: async () => '{"limit":0}' })).payload)
      .toMatchObject({ issues: expect.any(Array) });
    expect((await executeAwarenessCli(['schema', 'validate', 'memory_recall', '-'], { readStdin: async () => '{"query":"cli"}' })))
      .toMatchObject({ exitCode: 0, payload: { ok: true, schema: 'memory_recall' } });
    expect((await executeAwarenessCli(['schema', 'not-a-route', 'memory_recall', '--compact'])).payload)
      .toMatchObject({ error: expect.stringContaining('unknown command') });
  });

  it('returns executable canonical list continuations in compact and full modes', async () => {
    const root = tempRoot();
    const db = join(root, 'awareness.sqlite3');
    const bindings = ['--db', db, '--workspace', root, '--agent-id', 'owner'];
    const created = await executeAwarenessCli([
      'work', 'create', ...bindings, '--kind', 'standalone', '--rationale', 'coverage', '--test-plan', 'focused test',
      ...Array.from({ length: 7 }, (_, index) => ['--file', `file-${index}.ts`]).flat(), '--compact',
    ]);
    expect(created.exitCode, JSON.stringify(created.payload)).toBe(0);

    for (const full of [false, true]) {
      const first = await executeAwarenessCli(['work', 'list', ...bindings, '--kind', 'presence', '--limit', '2', ...(full ? ['--full'] : []), '--compact']);
      expect(first.exitCode, JSON.stringify(first.payload)).toBe(0);
      const payload = first.payload as { partial: boolean; next: { list: { command: { name: string; args: string[] } } } };
      expect(payload.partial).toBe(true);
      expect(payload.next.list.command.name).toBe('work list');
      const next = await executeAwarenessCli(['work', 'list', ...payload.next.list.command.args]);
      expect(next.exitCode, JSON.stringify(next.payload)).toBe(0);
    }
  });
});
