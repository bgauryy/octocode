import test from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { fixture, data, typedCli } from './agent-fixture.mjs';

test('shared input validation rejects invalid windows and recipe discovery in CLI and MCP', () =>
  fixture(async ({ client, cwd, skill }) => {
    for (const [name, input] of [
      ['skill', { length: 20001 }],
      ['schema', { recipe: 'page-snapshot' }],
      ['open', { args: ['--port', '9222'], headless: false }],
    ]) {
      const reply = await client.callTool({ name, arguments: input });
      assert.equal(reply.isError, true);
      const cli = typedCli(skill, [name, '--input', '-', '--json'], {
        cwd,
        input: JSON.stringify(input),
      });
      assert.notEqual(cli.status, 0, cli.stdout);
      assert.equal(
        existsSync(join(cwd, '.octocode')),
        false,
        'invalid input spawned an engine capture'
      );
    }
    const cleanup = data(
      await client.callTool({
        name: 'cleanup',
        arguments: { port: 19499, dryRun: true },
      })
    );
    assert.equal(cleanup.ok, true);
  }));

test('missing requested measurement evidence fails through CLI and MCP', () =>
  fixture(async ({ client, cwd, skill }) => {
    const input = { args: ['--perf', join(cwd, 'missing.json')] };
    const reply = await client.callTool({
      name: 'measure-query',
      arguments: input,
    });
    assert.equal(reply.isError, true);
    assert.match(JSON.stringify(reply), /Cannot read measurement artifact/);
    const cli = typedCli(skill, ['measure-query', '--input', '-', '--json'], {
      cwd,
      input: JSON.stringify(input),
    });
    assert.notEqual(cli.status, 0);
    assert.match(cli.stdout + cli.stderr, /Cannot read measurement artifact/);
  }));

test('focused public schema agrees across transports and rejects conflicting connection inputs before captures', () =>
  fixture(async ({ client, cwd, skill }) => {
    for (const [name, input, expected] of [
      [
        'schema',
        { command: 'check', operation: 'extract' },
        /operation requires/,
      ],
      ['query', {}, /file is required/],
      [
        'run',
        {
          plan: { steps: [{ op: 'cdp', method: 'Runtime.enable' }] },
          connection: { target: 'one', targetUrl: 'two' },
        },
        /one target selector/,
      ],
      [
        'run',
        {
          plan: { steps: [{ op: 'cdp', method: 'Runtime.enable' }] },
          connection: { keepTab: true, closeTab: true },
        },
        /cannot both be true/,
      ],
    ]) {
      const reply = await client.callTool({ name, arguments: input });
      assert.equal(reply.isError, true);
      assert.match(JSON.stringify(reply), expected);
      assert.equal(existsSync(join(cwd, '.octocode')), false);
    }
    const input = { command: 'run', operation: 'extract' };
    const reply = data(
      await client.callTool({ name: 'schema', arguments: input })
    );
    const cli = typedCli(skill, ['schema', '--input', '-', '--json'], {
      cwd,
      input: JSON.stringify(input),
    });
    assert.equal(cli.status, 0, cli.stderr);
    assert.deepEqual(JSON.parse(cli.stdout).structuredContent.data, reply.data);
    assert.deepEqual(reply.data.plan.extraction.fields, [
      'text',
      'href',
      'value',
      'role',
      'name',
    ]);
    for (const [name, example] of Object.entries(reply.data.inputExamples)) {
      if (name !== 'run') continue;
      const validated = data(
        await client.callTool({
          name,
          arguments: {
            ...example,
            connection: { ...example.connection, dryRun: true },
          },
        })
      );
      assert.equal(validated.ok, true);
    }
  }));

test('MCP exact numeric continuations retain predicate lexemes; typed unsafe numbers reject', async () =>
  fixture(async ({ client, cwd }) => {
    const file = join(cwd, 'exact.json');
    writeFileSync(
      file,
      '[{"id":"below","value":9007199254740992},{"id":"equal","value":9007199254740993},{"id":"above","value":9007199254740994}]'
    );
    let result = data(
      await client.callTool({
        name: 'query',
        arguments: {
          args: [
            '--file',
            file,
            '--where',
            '[{"path":"/value","op":"gte","value":9007199254740993}]',
            '--limit',
            '1',
          ],
        },
      })
    );
    assert.equal(result.ok, true);
    assert.equal(result.data.matched, 2);
    assert.equal(result.data.rows[0].value.id, 'equal');
    const route = result.data.next;
    assert(route.query.args, 'numeric filter must retain exact CLI text');
    result = data(
      await client.callTool({ name: route.tool, arguments: route.query })
    );
    assert.equal(result.ok, true);
    assert.deepEqual(
      result.data.rows.map(row => row.value.id),
      ['above']
    );
    const unsafe = await client.callTool({
      name: 'query',
      arguments: {
        file,
        where: [{ path: '/value', op: 'gte', value: 9007199254740993 }],
      },
    });
    assert.equal(unsafe.isError, true);
    assert.match(JSON.stringify(unsafe), /integer string/);
  }));

test('typed numeric predicates compare source decimals exactly through MCP and typed CLI', async () =>
  fixture(async ({ client, cwd, skill }) => {
    const file = join(cwd, 'numeric-boundaries.json');
    writeFileSync(
      file,
      '[{"id":"above","n":1.0000000000000001},{"id":"below","n":0.9999999999999999},{"id":"one","n":1},{"id":"huge","n":1e10000},{"id":"tiny","n":1e-10000}]'
    );
    for (const [op, value, expected] of [
      ['eq', 1, ['one']],
      ['gte', 1, ['above', 'one', 'huge']],
      ['lte', 1, ['below', 'one', 'tiny']],
      ['eq', 0, []],
    ]) {
      const where = [{ path: '/n', op, value }];
      const mcp = data(
        await client.callTool({ name: 'query', arguments: { file, where } })
      );
      const cli = typedCli(
        skill,
        ['query', '--file', file, '--where', JSON.stringify(where), '--json'],
        { cwd, timeout: 10000 }
      );
      assert.equal(cli.status, 0, cli.stderr);
      for (const response of [mcp, JSON.parse(cli.stdout).structuredContent]) {
        assert.equal(response.ok, true, JSON.stringify(response));
        assert.deepEqual(
          response.data.rows.map(row => row.value.id),
          expected,
          `${op} ${value}`
        );
        assert.equal(response.data.scanned, 5);
      }
    }
  }));
