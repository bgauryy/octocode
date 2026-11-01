import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import {
  connectStdio,
  cliFromMcp,
  runCli,
  spec,
  researchSpec,
} from '../../dist/runtime.js';
import { chromeCommands } from '../../dist/engine/chrome-contract.mjs';
import { fixture, data, typedCli } from './agent-fixture.mjs';

test('copied standalone MCP handshakes and exports the entire CLI registry', async () =>
  fixture(async ({ client }) => {
    const tools = (await client.listTools()).tools;
    assert.deepEqual(
      tools.map(tool => tool.name),
      Object.keys(chromeCommands)
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
    const help = typedCli(skill, ['run', '--help', '--json'], { cwd });
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
    const cli = typedCli(skill, ['run', '--input', file, '--json'], { cwd });
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
    // The typed plan refuses an empty plan before the engine runs.
    const refused = await client.callTool({
      name: 'run',
      arguments: { plan: { steps: [] } },
    });
    assert.equal(refused.isError, true);
    assert.match(refused.content[0].text, /steps/);
    // A typed plan the executor rejects keeps the engine's evidence.
    const failure = await client.callTool({
      name: 'run',
      arguments: { plan: { steps: [{ op: 'waitEvent', listener: 'none' }] } },
    });
    assert.equal(failure.isError, true);
    assert.equal(data(failure).exitCode, 2);
    assert.match(data(failure).error, /earlier listener/);
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
      await new Promise(done => setTimeout(done, 20));
    assert(existsSync(join(cwd, 'child.pid')));
    await client.close();
    await pending;
    while (!existsSync(join(cwd, 'stopped')) && Date.now() < until)
      await new Promise(done => setTimeout(done, 20));
    assert.equal(readFileSync(join(cwd, 'stopped'), 'utf8'), 'yes');
  }));

test('research preset exports exactly the research registry with typed filters and leaves the default registry whole', async () => {
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
    const names = tools.map(tool => tool.name);
    assert.deepEqual(
      new Set(names),
      new Set(researchSpec.commands.map(command => command.name))
    );
    assert.equal(names.length, new Set(names).size);
    assert(names.length < Object.keys(chromeCommands).length);
    assert(names.every(name => Object.hasOwn(chromeCommands, name)));
    assert(!names.includes('cookies'));
    assert(
      tools.find(tool => tool.name === 'query').inputSchema.properties.where
    );
    assert(
      tools.find(tool => tool.name === 'artifact').inputSchema.properties.file
    );
    assert.deepEqual(
      spec.commands.map(command => command.name),
      Object.keys(chromeCommands)
    );
  } finally {
    await client.close();
  }
});
