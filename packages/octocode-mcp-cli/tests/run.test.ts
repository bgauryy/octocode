import { afterEach, describe, expect, it } from 'vitest';
import { z } from 'zod';
import { cliView, runCli, type CliIo } from '../src/run.js';
import { defineCli, defineCommand } from '../src/spec.js';

const instructions = 'Use the index before a command.';

function io(): CliIo & { out: () => string; err: () => string } {
  const stdout: string[] = [];
  const stderr: string[] = [];
  return {
    stdout: text => { stdout.push(text); },
    stderr: text => { stderr.push(text); },
    out: () => stdout.join(''),
    err: () => stderr.join(''),
  };
}

const search = defineCommand({
  name: 'search',
  description: 'Search issues by text.',
  schema: z.object({
    query: z.string().min(2).describe('Search text'),
    limit: z.number().int().optional(),
    ratio: z.number().optional(),
    state: z.enum(['open', 'closed']).optional(),
    active: z.boolean().optional(),
    labels: z.array(z.string()).optional(),
    counts: z.array(z.number().int()).optional(),
    scores: z.array(z.number()).optional(),
    bits: z.array(z.boolean()).optional(),
    payload: z.unknown().optional(),
    force: z.boolean().default(false),
  }),
  run: input => input,
});

const spec = defineCli({
  name: 'issues',
  instructions,
  version: '1.2.3',
  commands: [
    search,
    defineCommand({ name: 'plain', description: 'Plain', schema: z.object({}), run: () => 'ok' }),
    defineCommand({ name: 'lined', description: 'Lined', schema: z.object({}), run: () => 'ok\n' }),
    defineCommand({ name: 'empty', description: 'Empty', schema: z.object({}), run: () => undefined }),
    defineCommand({ name: 'boom', description: 'Boom', schema: z.object({}), run: () => { throw new Error('boom'); } }),
    defineCommand({ name: 'textboom', description: 'Text boom', schema: z.object({}), run: () => { throw 'bad'; } }),
    defineCommand({
      name: 'color',
      description: 'Color',
      schema: z.object({}),
      run: () => '\u001b[31mred\u001b[0m',
    }),
    defineCommand({
      name: 'both',
      description: 'Both',
      schema: z.object({ query: z.string(), limit: z.number().int() }),
      run: input => input,
    }),
    defineCommand({
      name: 'rows',
      description: 'Rows',
      schema: z.object({}),
      run: () => [{ name: 'a', n: 1 }, { name: 'b', n: 2 }],
    }),
    defineCommand({
      name: 'spaced',
      description: 'Spaced',
      schema: z.object({}),
      run: () => [{ name: 'a ', n: '1 ' }, { name: 'b', n: '2' }],
    }),
    defineCommand({
      name: 'shaped',
      description: 'Shaped',
      schema: z.object({}),
      run: () => ({
        name: 'ada',
        n: 0,
        ok: false,
        empty: null,
        tags: ['a', 'b'],
        none: [],
        meta: { id: 'x' },
        only: [{ id: 'one' }],
        mix: [1, { id: 'a' }],
        messy: [{ id: 'a', child: { n: 1 } }, { id: 'b', child: { n: 2 } }],
        skip: undefined,
      }),
    }),
    defineCommand({
      name: 'blank',
      description: 'Blank',
      schema: z.object({}),
      run: () => [],
    }),
    defineCommand({
      name: 'view',
      description: 'View',
      schema: z.object({}),
      run: () => cliView('ignored', {
        structuredContent: { ok: true, items: [{ id: 'a' }, { id: 'b' }], msg: '\u001b[31mred\u001b[0m' },
        isError: false,
      }),
    }),
    defineCommand({
      name: 'task',
      description: 'Task',
      schema: z.object({}),
      run: () => cliView('{"ok":true}', { toolResult: { ok: true } }),
    }),
  ],
});

describe('runCli', () => {
  const originalOut = process.stdout.write;
  const originalErr = process.stderr.write;
  afterEach(() => {
    process.stdout.write = originalOut;
    process.stderr.write = originalErr;
  });

  it('prints help on stdout and keeps errors off the help page', async () => {
    const bare = io();
    expect(await runCli(spec, [], bare)).toBe(0);
    expect(bare.out().split(instructions)).toHaveLength(2);
    expect(bare.out()).toContain('USAGE');
    expect(bare.out()).toContain('COMMANDS');
    expect(bare.out()).not.toContain('\nFLAGS\n');
    expect(bare.err()).toBe('');

    const help = io();
    expect(await runCli(spec, ['--help'], help)).toBe(0);
    expect(help.out()).toContain('FLAGS');
    expect(help.out()).toContain('LEARN MORE');
    expect(help.err()).toBe('');
    expect(await runCli(spec, ['-h'], io())).toBe(0);

    const commandHelp = io();
    expect(await runCli(spec, ['search', '--help', '--query'], commandHelp)).toBe(0);
    expect(commandHelp.out().startsWith('Search issues by text.')).toBe(true);
    expect(commandHelp.out()).toContain('issues search --query <string>');
    expect(commandHelp.out()).not.toContain(instructions);
    expect(commandHelp.err()).toBe('');

    const json = io();
    expect(await runCli(spec, ['search', '--help', '--json'], json)).toBe(0);
    const parsed = JSON.parse(json.out()) as { flags: Array<{ name: string }> };
    expect(parsed.flags.map(flag => flag.name)).toEqual(search.flags.map(flag => flag.name));

    const named = io();
    expect(await runCli(spec, ['help', 'search'], named)).toBe(0);
    expect(named.out().startsWith('Search issues by text.')).toBe(true);
    const namedJson = io();
    expect(await runCli(spec, ['help', 'search', '--json'], namedJson)).toBe(0);
    expect(JSON.parse(namedJson.out()).description).toBe('Search issues by text.');
    const rootJson = io();
    expect(await runCli(spec, ['--help', '--json'], rootJson)).toBe(0);
    expect(JSON.parse(rootJson.out())).toMatchObject({ instructions });
    const unknown = io();
    expect(await runCli(spec, ['nope', '--help'], unknown)).toBe(2);
    expect(unknown.err()).toContain('Unknown command: nope');
  });

  it('exits 2 with the scheme when a required flag is missing', async () => {
    const missing = io();
    expect(await runCli(spec, ['search'], missing)).toBe(2);
    expect(missing.out()).toContain('missing required flag: --query');
    expect(missing.out()).toContain('--query <string>');
    expect(missing.err()).toBe('');
    const many = io();
    expect(await runCli(spec, ['both'], many)).toBe(2);
    expect(many.out()).toContain('missing required flag: --query, --limit');
    expect(many.err()).toBe('');
  });

  it('writes handler results to stdout and failures to stderr', async () => {
    const before = process.exitCode;
    const ok = io();
    expect(await runCli(spec, ['search', '--query', 'hi', '--force', '--limit', '-3', '--labels', 'a', '--labels', 'b'], ok)).toBe(0);
    expect(ok.out()).toContain('query: hi\n');
    expect(ok.out()).toContain('force: true\n');
    expect(ok.out()).toContain('limit: -3\n');
    expect(ok.out()).toContain('labels: a, b\n');
    expect(ok.out()).not.toContain('{');
    expect(ok.err()).toBe('');
    expect(process.exitCode).toBe(before);

    const failed = io();
    expect(await runCli(spec, ['boom'], failed)).toBe(1);
    expect(failed.err()).toBe('boom\n');
    expect(failed.out()).toBe('');
    const text = io();
    expect(await runCli(spec, ['textboom'], text)).toBe(1);
    expect(text.err()).toBe('bad\n');

    const plain = io();
    expect(await runCli(spec, ['plain'], plain)).toBe(0);
    expect(plain.out()).toBe('ok\n');
    expect((io() && await (async () => {
      const lined = io();
      expect(await runCli(spec, ['lined'], lined)).toBe(0);
      expect(lined.out()).toBe('ok\n');
      return lined.out();
    })())).toBe('ok\n');

    const empty = io();
    expect(await runCli(spec, ['empty'], empty)).toBe(0);
    expect(empty.out()).toBe('');
    const emptyJson = io();
    expect(await runCli(spec, ['empty', '--json'], emptyJson)).toBe(0);
    expect(emptyJson.out()).toBe('');
    const asJson = io();
    expect(await runCli(spec, ['plain', '--json'], asJson)).toBe(0);
    expect(asJson.out()).toBe('"ok"\n');
    const color = io();
    expect(await runCli(spec, ['color', '--no-color', '--no-input'], color)).toBe(0);
    expect(color.out()).toBe('red\n');
    const colored = io();
    expect(await runCli(spec, ['color'], colored)).toBe(0);
    expect(colored.out()).toContain('\u001b[31m');

    const rows = io();
    expect(await runCli(spec, ['rows'], rows)).toBe(0);
    expect(rows.out()).toBe('name  n\na     1\nb     2\n');
    const spaced = io();
    expect(await runCli(spec, ['spaced'], spaced)).toBe(0);
    expect(spaced.out()).toBe('name  n\na     1 \nb     2\n');
    const shaped = io();
    expect(await runCli(spec, ['shaped'], shaped)).toBe(0);
    expect(shaped.out()).toBe([
      'name: ada',
      'n: 0',
      'ok: false',
      'empty: null',
      'tags: a, b',
      'none: (none)',
      'meta:',
      '  id: x',
      'only:',
      '  id: one',
      'mix:',
      '  1',
      '  id: a',
      'messy:',
      '  id: a',
      '  child:',
      '    n: 1',
      '',
      '  id: b',
      '  child:',
      '    n: 2',
      '',
    ].join('\n'));
    const blank = io();
    expect(await runCli(spec, ['blank'], blank)).toBe(0);
    expect(blank.out()).toBe('(none)\n');
    const viewed = io();
    expect(await runCli(spec, ['view'], viewed)).toBe(0);
    expect(viewed.out()).toBe('ok: true\nitems:\n  id\n  a\n  b\nmsg: \u001b[31mred\u001b[0m\n');
    const faded = io();
    expect(await runCli(spec, ['view', '--no-color'], faded)).toBe(0);
    expect(faded.out()).toBe('ok: true\nitems:\n  id\n  a\n  b\nmsg: red\n');
    const task = io();
    expect(await runCli(spec, ['task'], task)).toBe(0);
    expect(task.out()).toBe('{"ok":true}\n');
    const taskJson = io();
    expect(await runCli(spec, ['task', '--json'], taskJson)).toBe(0);
    expect(JSON.parse(taskJson.out())).toEqual({ toolResult: { ok: true } });
  });

  it('reports usage errors without dumping help on stderr', async () => {
    const unknown = io();
    expect(await runCli(spec, ['nope'], unknown)).toBe(2);
    expect(unknown.err()).toBe('Unknown command: nope\n');
    expect(unknown.out()).not.toContain('USAGE');

    const flag = io();
    expect(await runCli(spec, ['search', '--query', 'hi', '--extra'], flag)).toBe(2);
    expect(flag.err()).toBe('Unknown flag: --extra\n');
    expect(flag.out()).toBe('');

    const invalid = io();
    expect(await runCli(spec, ['search', '--query', 'a'], invalid)).toBe(2);
    expect(invalid.err()).toMatch(/query:/);
    expect(invalid.out()).toBe('');

    expect((await (async () => {
      const streams = io();
      await runCli(spec, ['search', '--query'], streams);
      return streams.err();
    })())).toContain('Missing value');
    const missingNext = io();
    expect(await runCli(spec, ['search', '--query', '--limit'], missingNext)).toBe(2);
    expect(missingNext.err()).toContain('Missing value for --query');
    const badInt = io();
    expect(await runCli(spec, ['search', '--query', 'hi', '--limit', '1.5'], badInt)).toBe(2);
    expect(badInt.err()).toContain('Invalid integer');
    const badNum = io();
    expect(await runCli(spec, ['search', '--query', 'hi', '--ratio', 'no'], badNum)).toBe(2);
    expect(badNum.err()).toContain('Invalid number');
    const badBool = io();
    expect(await runCli(spec, ['search', '--query', 'hi', '--active', 'maybe'], badBool)).toBe(2);
    expect(badBool.err()).toContain('Invalid boolean');
    const badEnum = io();
    expect(await runCli(spec, ['search', '--query', 'hi', '--state', 'no'], badEnum)).toBe(2);
    expect(badEnum.err()).toContain('Invalid value');
    const badJson = io();
    expect(await runCli(spec, ['search', '--query', 'hi', '--payload', '{'], badJson)).toBe(2);
    expect(badJson.err()).toContain('Invalid JSON');
    const value = io();
    expect(await runCli(spec, ['search', '--json', '--query=hi', '--state', 'open', '--active', 'false', '--ratio', '1.5', '--payload', '{"a":1}', '--counts', '-2', '--scores', '-1.5', '--bits', 'true', '--'], value)).toBe(0);
    expect(JSON.parse(value.out())).toMatchObject({
      query: 'hi',
      state: 'open',
      active: false,
      ratio: 1.5,
      payload: { a: 1 },
      counts: [-2],
      scores: [-1.5],
      bits: [true],
    });
    const extra = io();
    expect(await runCli(spec, ['search', '--query', 'hi', 'extra'], extra)).toBe(2);
    expect(extra.err()).toContain('Unexpected argument');
    const after = io();
    expect(await runCli(spec, ['search', '--query', 'hi', '--', 'more'], after)).toBe(2);
    expect(after.err()).toContain('Unexpected argument: more');
    const presence = io();
    expect(await runCli(spec, ['search', '--query', 'hi', '--force=true'], presence)).toBe(2);
    expect(presence.err()).toContain('does not take a value');
    const global = io();
    expect(await runCli(spec, ['search', '--query', 'hi', '--json=1'], global)).toBe(2);
    expect(global.err()).toContain('does not take a value');
    const short = io();
    expect(await runCli(spec, ['search', '--query', 'hi', '-x'], short)).toBe(2);
    expect(short.err()).toContain('Unknown flag: -x');
    const helpMiss = io();
    expect(await runCli(spec, ['help', 'missing'], helpMiss)).toBe(2);
    expect(helpMiss.err()).toContain('Unknown command: missing');
    const lone = io();
    expect(await runCli(spec, ['--weird'], lone)).toBe(2);
    expect(lone.err()).toContain('Unknown flag: --weird');
    const version = io();
    expect(await runCli(spec, ['search', '--version'], version)).toBe(0);
    expect(version.out()).toBe('1.2.3\n');
    const indexed = io();
    expect(await runCli(spec, ['--json'], indexed)).toBe(0);
    expect(JSON.parse(indexed.out())).toMatchObject({ instructions });
    const helped = io();
    expect(await runCli(spec, ['help'], helped)).toBe(0);
    expect(helped.out()).toContain('FLAGS');
    expect(helped.out()).toContain('LEARN MORE');
  });

  it('uses process stdout and stderr when io is omitted', async () => {
    const out: string[] = [];
    const err: string[] = [];
    process.stdout.write = ((chunk: string | Uint8Array) => {
      out.push(typeof chunk === 'string' ? chunk : Buffer.from(chunk).toString());
      return true;
    }) as typeof process.stdout.write;
    process.stderr.write = ((chunk: string | Uint8Array) => {
      err.push(typeof chunk === 'string' ? chunk : Buffer.from(chunk).toString());
      return true;
    }) as typeof process.stderr.write;
    expect(await runCli(spec, ['--version'])).toBe(0);
    expect(out.join('')).toBe('1.2.3\n');
    expect(await runCli(spec, ['nope'])).toBe(2);
    expect(err.join('')).toContain('Unknown command: nope');
  });
  it('accepts lossless JSON arrays and still validates each item', async () => {
    const result = io();
    expect(await runCli(spec, ['search', '--query', 'test', '--counts=[1,2]', '--labels=[]'], result)).toBe(0);
    expect(result.out()).toContain('1');
    expect(await runCli(spec, ['search', '--query', 'test', '--counts=["bad"]'], io())).toBe(2);
    expect(await runCli(spec, ['search', '--query', 'test', '--labels=[literal'], io())).toBe(0);
  });

  it('preserves an explicit subprocess exit status', async () => {
    const command = defineCommand({ name: 'interrupted', description: 'Interrupted', schema: z.object({}), run: () => { throw Object.assign(new Error('interrupted'), { exitCode: 130 }); } });
    expect(await runCli(defineCli({ name: 'test', instructions: '', commands: [command] }), ['interrupted'], io())).toBe(130);
  });

});
