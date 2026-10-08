import test from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdtempSync,
  cpSync,
  rmSync,
  readFileSync,
  writeFileSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { connectStdio, cliFromMcp, runCli, spec } from '../../dist/runtime.js';
import { commands } from '../../dist/engine/cli-catalog.mjs';

async function fixture(fn) {
  const work = mkdtempSync(join(tmpdir(), 'octo-agent-'));
  const skill = join(work, 'skill');
  cpSync(resolve(import.meta.dirname, '../..'), skill, {
    recursive: true,
    filter: source => !source.split(/[\\/]/).includes('node_modules'),
  });
  const cwd = join(work, 'workspace');
  cpSync(join(skill, 'package.json'), join(work, 'package.json'));
  const { mkdirSync } = await import('node:fs');
  mkdirSync(cwd);
  const client = await connectStdio({
    command: process.execPath,
    args: [join(skill, 'bin/octocode-chrome-devtools.mjs')],
    cwd,
  });
  try {
    await fn({ client, cwd, skill, work });
  } finally {
    await client.close();
    rmSync(work, { recursive: true, force: true });
  }
}
const data = result =>
  result.structuredContent ??
  JSON.parse(result.content.find(row => row.type === 'text').text);
test('copied standalone MCP handshakes and exports the entire CLI registry', async () =>
  fixture(async ({ client }) => {
    const tools = (await client.listTools()).tools;
    assert.deepEqual(
      tools.map(tool => tool.name),
      Object.keys(commands)
    );
    assert.match(client.getInstructions(), /Plans stop at failure/);
    const command = tools.find(tool => tool.name === 'cdp');
    assert(command.inputSchema.properties.method);
    assert(command.inputSchema.properties.connection);
    assert.equal(
      tools.find(tool => tool.name === 'cdp').annotations.readOnlyHint,
      false
    );
  }));
test('typed CLI help is standalone and creates no workspace state', async () =>
  fixture(async ({ cwd, skill }) => {
    const help = spawnSync(
      process.execPath,
      [
        join(skill, 'bin/octocode-chrome-devtools.mjs'),
        '/cli',
        'run',
        '--help',
        '--json',
      ],
      { cwd, encoding: 'utf8' }
    );
    assert.equal(help.status, 0, help.stderr);
    assert(JSON.parse(help.stdout).inputSchema.properties.plan);
    assert.equal(existsSync(join(cwd, '.octocode')), false);
  }));
test('malformed tool schemas and plans fail without contacting Chrome', async () =>
  fixture(async ({ client }) => {
    for (const [name, input] of [
      ['cdp', { method: 'Runtime.evaluate', connection: { port: 0 } }],
      ['run', { plan: { steps: [] } }],
      ['targets', { extra: true }],
    ]) {
      const result = await client.callTool({ name, arguments: input });
      assert.equal(result.isError, true);
      assert.doesNotMatch(result.content[0].text, /ECONNREFUSED/);
    }
  }));
test('core typed CLI and MCP dry-run plans produce identical invocations', async () =>
  fixture(async ({ client, cwd, skill, work }) => {
    const input = {
      plan: {
        steps: [
          {
            op: 'cdp',
            method: 'Runtime.evaluate',
            params: { expression: '42' },
          },
        ],
      },
      connection: { dryRun: true, port: 9999 },
    };
    const mcp = data(await client.callTool({ name: 'run', arguments: input }));
    const file = join(work, 'input.json');
    writeFileSync(file, JSON.stringify(input));
    const cli = spawnSync(
      process.execPath,
      [
        join(skill, 'bin/octocode-chrome-devtools.mjs'),
        '/cli',
        'run',
        '--input',
        file,
        '--json',
      ],
      { cwd, encoding: 'utf8' }
    );
    assert.equal(cli.status, 0, cli.stderr);
    const typed = JSON.parse(cli.stdout).structuredContent;
    assert.deepEqual(typed.data, mcp.data);
  }));
test('MCP-to-CLI discovery and success retain structure; MCP failures retain evidence', async () =>
  fixture(async ({ client }) => {
    const imported = await cliFromMcp(client);
    assert.equal(imported.commands.length, spec.commands.length);
    const output = [],
      errors = [];
    const ok = await runCli(
      imported,
      ['schema', '--command', 'run', '--json'],
      { stdout: text => output.push(text), stderr: text => errors.push(text) }
    );
    assert.equal(ok, 0);
    assert(JSON.parse(output.join('')).structuredContent.ok);
    assert.equal(errors.length, 0);
    const failure = await client.callTool({
      name: 'run',
      arguments: { plan: { steps: [] } },
    });
    assert.equal(failure.isError, true);
    assert.equal(data(failure).exitCode, 2);
    assert.match(data(failure).error, /non-empty steps/);
  }));
test('large MCP output is bounded and its continuation reconstructs every byte', async () =>
  fixture(async ({ client }) => {
    const plan = {
      steps: [
        {
          op: 'cdp',
          method: 'Runtime.evaluate',
          params: { expression: 'x'.repeat(30000) },
        },
      ],
    };
    const result = data(
      await client.callTool({
        name: 'run',
        arguments: { plan, connection: { dryRun: true } },
      })
    );
    assert(Buffer.byteLength(JSON.stringify(result)) <= 16000);
    assert(result.next.stdout);
    const expected = readFileSync(result.logs.stdout, 'utf8');
    let next = result.next.stdout,
      restored = '';
    for (let page = 0; next; page++) {
      assert(page < 100, 'Continuation did not terminate');
      const response = data(
        await client.callTool({ name: next.tool, arguments: next.query })
      );
      assert(response.ok);
      assert(response.data, 'Reader page must fit directly');
      const part = response.data;
      restored += part.text ?? part.content ?? '';
      next = part.next;
    }
    assert.equal(restored, expected);
  }));
test('concurrent MCP requests serialize engine execution', async () =>
  fixture(async ({ client, skill }) => {
    writeFileSync(
      join(skill, 'dist/engine/cli.mjs'),
      'const started=Date.now();setTimeout(()=>console.log(JSON.stringify({started,finished:Date.now()})),100);'
    );
    const results = await Promise.all([
      client.callTool({ name: 'targets', arguments: {} }),
      client.callTool({ name: 'targets', arguments: {} }),
    ]);
    const times = results.map(result => data(result).data);
    assert(times[1].started >= times[0].finished);
  }));
test('closing the MCP session cancels the active CLI child', async () =>
  fixture(async ({ client, skill, cwd }) => {
    writeFileSync(
      join(skill, 'dist/engine/cli.mjs'),
      "import {writeFileSync} from 'node:fs';writeFileSync('child.pid',String(process.pid));process.on('SIGTERM',()=>{writeFileSync('stopped','yes');process.exit(0);});setInterval(()=>{},1000);"
    );
    const pending = client
      .callTool({ name: 'targets', arguments: {} })
      .catch(() => {});
    const until = Date.now() + 3000;
    while (!existsSync(join(cwd, 'child.pid')) && Date.now() < until)
      await new Promise(resolve => setTimeout(resolve, 20));
    assert(existsSync(join(cwd, 'child.pid')));
    await client.close();
    await pending;
    while (!existsSync(join(cwd, 'stopped')) && Date.now() < until)
      await new Promise(resolve => setTimeout(resolve, 20));
    assert.equal(readFileSync(join(cwd, 'stopped'), 'utf8'), 'yes');
  }));

test('research preset exports ten tools with typed filters and preserves the full default registry', async () => {
  const client = await connectStdio({
    command: process.execPath,
    args: [
      resolve(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs'),
      '--preset',
      'research',
    ],
  });
  try {
    const tools = (await client.listTools()).tools;
    assert.equal(tools.length, 10);
    assert(!tools.some(tool => tool.name === 'cookies'));
    assert(
      tools.find(tool => tool.name === 'query').inputSchema.properties.where
    );
    assert(
      tools.find(tool => tool.name === 'artifact').inputSchema.properties.file
    );
    assert.equal(spec.commands.length, 25);
  } finally {
    await client.close();
  }
});

test('shared input validation rejects invalid windows and recipe discovery in CLI and MCP', () =>
  fixture(async ({ client, cwd, skill }) => {
    for (const [name, input] of [
      ['skill', { length: 20001 }],
      ['schema', { recipe: 'page-snapshot' }],
      ['open', { args: ['--port', '9222'], headless: false }],
    ]) {
      const reply = await client.callTool({ name, arguments: input });
      assert.equal(reply.isError, true);
      const cli = spawnSync(
        process.execPath,
        [
          join(skill, 'bin/octocode-chrome-devtools.mjs'),
          '/cli',
          name,
          '--input',
          '-',
          '--json',
        ],
        {
          cwd,
          encoding: 'utf8',
          input: JSON.stringify(input),
        }
      );
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
    const cli = spawnSync(
      process.execPath,
      [
        join(skill, 'bin/octocode-chrome-devtools.mjs'),
        '/cli',
        'measure-query',
        '--input',
        '-',
        '--json',
      ],
      {
        cwd,
        encoding: 'utf8',
        input: JSON.stringify(input),
      }
    );
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
    const cli = spawnSync(
      process.execPath,
      [
        join(skill, 'bin/octocode-chrome-devtools.mjs'),
        '/cli',
        'schema',
        '--input',
        '-',
        '--json',
      ],
      { cwd, encoding: 'utf8', input: JSON.stringify(input) }
    );
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
