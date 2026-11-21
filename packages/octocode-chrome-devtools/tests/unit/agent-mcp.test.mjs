import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import {
  connectStdio,
  cliFromMcp,
  runCli,
  spec,
  researchSpec,
  presets,
} from '../../dist/runtime.js';
import { chromeCommands } from '../../dist/engine/chrome-contract.mjs';
import { fixture, data, typedCli } from './agent-fixture.mjs';

const bin = resolve(
  import.meta.dirname,
  '../../bin/octocode-chrome-devtools.mjs'
);
const listNames = async args => {
  const client = await connectStdio({
    command: process.execPath,
    args: [bin, ...args],
  });
  try {
    return { client, tools: (await client.listTools()).tools };
  } catch (error) {
    await client.close();
    throw error;
  }
};
test('default MCP surface is the research preset; slim is a subset of it and --preset full exports the registry except the CLI-only snapshot and screenshot', async () => {
  const research = await listNames([]);
  try {
    assert.deepEqual(
      research.tools.map(tool => tool.name),
      researchSpec.commands.map(command => command.name)
    );
    assert(research.tools.length < Object.keys(chromeCommands).length);
    assert.match(
      research.client.getInstructions(),
      /plans stop at the first failing step/
    );
  } finally {
    await research.client.close();
  }
  const slim = await listNames(['--mcp', '--preset', 'slim']);
  try {
    const names = slim.tools.map(tool => tool.name);
    assert.deepEqual(
      names,
      presets.slim.commands.map(command => command.name)
    );
    const researchNames = researchSpec.commands.map(command => command.name);
    assert(names.length < researchNames.length);
    for (const name of names) assert(researchNames.includes(name), name);
  } finally {
    await slim.client.close();
  }
  const full = await listNames(['--preset', 'full']);
  try {
    assert.deepEqual(
      full.tools.map(tool => tool.name),
      presets.full.commands.map(command => command.name)
    );
    assert(!full.tools.some(tool => tool.name === 'snapshot'));
    const command = full.tools.find(tool => tool.name === 'cdp');
    assert(command.inputSchema.properties.method);
    assert(command.inputSchema.properties.connection);
    assert.equal(command.annotations.readOnlyHint, false);
  } finally {
    await full.client.close();
  }
});
test('an unknown MCP preset is refused with usage text naming every preset', async () => {
  const result = spawnSync(process.execPath, [bin, '--preset', 'nope'], {
    encoding: 'utf8',
    timeout: 10000,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Usage/);
  for (const preset of Object.keys(presets))
    assert.match(result.stderr, new RegExp(preset));
});
test('success text is the one-line compact reply itself; failure text leads with cause and next call', async () =>
  fixture(async ({ client }) => {
    const ok = await client.callTool({
      name: 'schema',
      arguments: { command: 'run' },
    });
    assert.notEqual(ok.isError, true);
    const text = ok.content[0].text;
    assert(!text.includes('\n'));
    // A text-only client sees the whole reply: the text is the structured content.
    assert.deepEqual(JSON.parse(text), ok.structuredContent);
    assert.equal(ok.structuredContent.ok, true);
    for (const dropped of [
      'exitCode',
      'signal',
      'isPartial',
      'capture',
      'logs',
    ])
      assert(!(dropped in ok.structuredContent), dropped);
    const failure = await client.callTool({
      name: 'run',
      arguments: { plan: { steps: [{ op: 'waitEvent', listener: 'none' }] } },
    });
    assert.equal(failure.isError, true);
    const body = JSON.parse(failure.content[0].text);
    assert.match(body.summary, /^Failed: .*earlier listener/);
    assert(body.summary.length < 700);
    assert.equal(Object.keys(body)[0], 'summary');
  }));
test('flow save takes an opaque plan and the engine validates it before writing anything', async () =>
  fixture(async ({ client, cwd }) => {
    const flow = (await client.listTools()).tools.find(t => t.name === 'flow');
    assert.equal(flow.inputSchema.properties.plan.type, 'object');
    assert.equal(flow.inputSchema.properties.plan.properties, undefined);
    for (const plan of [{ steps: [] }, { steps: [{ op: 'nope' }] }]) {
      const result = await client.callTool({
        name: 'flow',
        arguments: { action: 'save', name: 'junk', plan },
      });
      assert.equal(result.isError, true, JSON.stringify(plan));
    }
    assert.equal(
      existsSync(join(cwd, '.octocode/octocode-chrome-devtools/flows')),
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
      ['schema', { extra: true }],
      ['run', { plan: { steps: [] } }],
      ['targets', { extra: true }],
    ]) {
      const result = await client.callTool({ name, arguments: input });
      assert.equal(result.isError, true);
      assert.doesNotMatch(result.content[0].text, /ECONNREFUSED/);
    }
  }));
test('core typed CLI and MCP report the same failure for a plan with no browser', async () =>
  fixture(async ({ client, cwd, skill, work }) => {
    const input = {
      plan: {
        steps: [
          {
            op: 'cdp',
            method: 'Runtime.evaluate',
            acknowledge: true,
            params: { expression: '42' },
          },
        ],
      },
      connection: { port: 9 },
    };
    const mcp = await client.callTool({ name: 'run', arguments: input });
    assert.equal(mcp.isError, true);
    const { error } = JSON.parse(mcp.content[0].text);
    assert.match(error, /No Chrome on port 9/);
    const file = join(work, 'input.json');
    writeFileSync(file, JSON.stringify(input));
    const cli = typedCli(skill, ['run', '--input', file, '--json'], { cwd });
    assert.notEqual(cli.status, 0);
    assert.ok((cli.stdout + cli.stderr).includes(error), cli.stderr);
  }));
test('a policy refusal fails with a small payload that keeps one continuation', async () =>
  fixture(async ({ client }) => {
    const refused = await client.callTool({
      name: 'cdp',
      arguments: { method: 'Browser.close' },
    });
    assert.equal(refused.isError, true);
    const text = refused.content[0].text;
    assert.ok(text.length < 700, `${text.length} chars`);
    const body = JSON.parse(text);
    assert.equal(Object.keys(body)[0], 'summary');
    assert.equal(body.ok, false);
    assert.equal(typeof body.exitCode, 'number');
    assert.ok(body.next.query.file, 'one executable continuation remains');
    for (const dropped of ['capture', 'logs', 'search', 'artifacts', 'root'])
      assert.equal(Object.hasOwn(body, dropped), false, dropped);
  }));
test('MCP-to-CLI discovery and success retain structure; MCP failures retain evidence', async () =>
  fixture(async ({ client }) => {
    const imported = await cliFromMcp(client);
    assert.equal(imported.commands.length, researchSpec.commands.length);
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
  fixture(async ({ client, cwd }) => {
    // The default schema document is the largest browser-free engine output.
    const result = data(
      await client.callTool({ name: 'schema', arguments: {} })
    );
    assert(Buffer.byteLength(JSON.stringify(result)) <= 16000);
    assert(result.next.stdout);
    const expected = readFileSync(
      join(cwd, result.next.stdout.query.file),
      'utf8'
    );
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

test('research preset exports exactly the research registry with typed filters and leaves the spec registry whole', async () => {
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
test('every tool declares read-only, destructive and idempotent hints, and read-only tools are idempotent', () =>
  fixture(async ({ client }) => {
    const { tools } = await client.listTools();
    for (const { name, annotations } of tools) {
      for (const hint of ['readOnlyHint', 'destructiveHint', 'idempotentHint'])
        assert.equal(typeof annotations?.[hint], 'boolean', `${name}.${hint}`);
      if (annotations.readOnlyHint)
        assert.equal(annotations.idempotentHint, true, name);
    }
  }));
