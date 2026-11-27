import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { connectStdio, presets, spec } from '../../dist/runtime.js';
import {
  chromeCommands,
  chromeRecipeOptionNames,
} from '../../dist/engine/chrome-contract.mjs';

// What clients load on connect: trimmed of tokens that carry no information, and honest about
// what each tool accepts. The command's own schema still validates every call.
const bin = resolve(
  import.meta.dirname,
  '../../bin/octocode-chrome-devtools.mjs'
);
const listed = new Map();
const tools = async preset => {
  if (listed.has(preset)) return listed.get(preset);
  const client = await connectStdio({
    command: process.execPath,
    args: [bin, '--mcp', '--preset', preset],
  });
  try {
    const { tools: listedTools } = await client.listTools();
    const result = { tools: listedTools };
    const calls = new Map();
    result.call = async (name, args) => {
      const key = `${name}:${JSON.stringify(args)}`;
      if (!calls.has(key))
        calls.set(key, await client.callTool({ name, arguments: args }));
      return calls.get(key);
    };
    // The client closes after this block, so the replies the tests read are fetched now (schema is in research and full).
    if (listedTools.some(tool => tool.name === 'schema')) {
      for (const args of [
        { command: 'run' },
        { command: 'snapshot' },
        { command: 'screenshot' },
        { command: 'check', recipe: 'nope' },
        ...Object.keys(chromeRecipeOptionNames).map(recipe => ({
          command: 'check',
          recipe,
        })),
      ])
        await result.call('schema', args);
    }
    listed.set(preset, result);
    return result;
  } finally {
    await client.close();
  }
};
const command = name => spec.commands.find(row => row.name === name);
const shape = (surface, name) =>
  surface.tools.find(tool => tool.name === name).inputSchema;
const reply = result =>
  result.structuredContent ?? JSON.parse(result.content[0].text);

test('no advertised input schema carries $schema or the safe-integer bounds', async () => {
  const unsafe = String(Number.MAX_SAFE_INTEGER);
  for (const preset of Object.keys(presets)) {
    const surface = await tools(preset);
    for (const tool of surface.tools) {
      const text = JSON.stringify(tool.inputSchema);
      assert.ok(!('$schema' in tool.inputSchema), `${preset} ${tool.name}`);
      assert.ok(!text.includes(unsafe), `${preset} ${tool.name}`);
    }
  }
});

test('every advertised property has a description', async () => {
  const surface = await tools('full');
  const missing = [];
  const walk = (node, path, defs) => {
    for (const [key, value] of Object.entries(node?.properties ?? {})) {
      const ref = value.$ref
        ? { ...defs[value.$ref.replace('#/$defs/', '')], ...value }
        : value;
      if (!ref.description) missing.push(`${path}.${key}`);
      walk(value, `${path}.${key}`, defs);
    }
    if (node?.items) walk(node.items, `${path}[]`, defs);
    for (const one of node?.anyOf ?? []) walk(one, path, defs);
  };
  for (const tool of surface.tools)
    walk(tool.inputSchema, tool.name, tool.inputSchema.$defs ?? {});
  assert.deepEqual(missing, []);
});

test('targets and protocol advertise only the port of their connection, and still accept the rest', async () => {
  const surface = await tools('research');
  for (const name of ['targets', 'protocol']) {
    const connection = shape(surface, name).properties.connection;
    assert.deepEqual(Object.keys(connection.properties), ['port'], name);
    assert.equal(
      command(name).schema.safeParse({
        connection: { port: 9222, target: 't' },
      }).success,
      true,
      name
    );
  }
  assert.ok('target' in shape(surface, 'run').properties.connection.properties);
});

test('run advertises a smaller plan; schema --command run still lists the dropped options, and a plan with them validates', async () => {
  const surface = await tools('research');
  const dropped = ['observe', 'traceEvents', 'commandMs', 'settleMs'];
  const advertised = shape(surface, 'run').properties.plan.properties;
  for (const key of dropped) assert.equal(advertised[key], undefined, key);
  const listing = JSON.stringify(
    reply(await surface.call('schema', { command: 'run' }))
  );
  for (const key of dropped) assert.ok(listing.includes(`"${key}"`), key);
  const step = { op: 'goto', url: 'https://example.com' };
  for (const plan of [
    { steps: [step], observe: { network: true, afterMs: 5 } },
    { steps: [step], traceEvents: true, commandMs: 5, settleMs: 0 },
  ])
    assert.equal(command('run').schema.safeParse({ plan }).success, true);
  assert.equal(
    command('run').schema.safeParse({
      plan: { steps: [step], observe: { network: 'yes' } },
    }).success,
    false
  );
});

test('no MCP preset exports snapshot, screenshot or artifact; the CLI registry keeps them and schema describes their options', async () => {
  for (const preset of Object.keys(presets)) {
    const names = (await tools(preset)).tools.map(tool => tool.name);
    for (const name of ['snapshot', 'screenshot', 'artifact'])
      assert.ok(!names.includes(name), `${preset} ${name}`);
  }
  const full = (await tools('full')).tools.map(tool => tool.name);
  assert.deepEqual(
    full,
    Object.keys(chromeCommands).filter(
      name => !['snapshot', 'screenshot', 'artifact'].includes(name)
    )
  );
  assert.deepEqual(
    spec.commands.map(row => row.name),
    Object.keys(chromeCommands)
  );
  const surface = await tools('research');
  for (const [name, recipe] of [
    ['snapshot', 'page-snapshot'],
    ['screenshot', 'page-screenshot'],
  ]) {
    const options = reply(await surface.call('schema', { command: name })).data
      .options;
    assert.deepEqual(
      Object.keys(options.fields),
      chromeRecipeOptionNames[recipe],
      name
    );
  }
});

test('schema.command lists only the tools of its preset, and recipe and operation are plain strings', async () => {
  for (const preset of Object.keys(presets)) {
    const surface = await tools(preset);
    const names = surface.tools.map(tool => tool.name);
    if (!names.includes('schema')) continue;
    const {
      command: field,
      recipe,
      operation,
    } = shape(surface, 'schema').properties;
    assert.deepEqual([...field.enum].sort(), [...names].sort(), preset);
    assert.equal(recipe.enum, undefined);
    assert.equal(operation.enum, undefined);
  }
  const refused = await (
    await tools('research')
  ).call('schema', { command: 'check', recipe: 'nope' });
  assert.equal(refused.isError, true);
  assert.match(JSON.stringify(refused), /page-snapshot/);
});

test('every check recipe has option guidance and an example that its own schema accepts', async () => {
  const surface = await tools('full');
  const enumerated = shape(surface, 'check').properties.recipe.enum;
  assert.deepEqual(
    [...enumerated].sort(),
    Object.keys(chromeRecipeOptionNames).sort()
  );
  for (const recipe of enumerated) {
    const guide = reply(
      await surface.call('schema', { command: 'check', recipe })
    ).data.checks[recipe];
    assert.ok(guide.example, recipe);
    assert.equal(
      command('check').schema.safeParse(guide.example).success,
      true,
      recipe
    );
  }
});

test('group takes its port as one top-level field and refuses connection', () => {
  const group = command('group').schema;
  const open = { action: 'list', port: 9222 };
  assert.equal(group.safeParse(open).success, true);
  assert.equal(
    group.safeParse({ action: 'list', connection: { port: 9222 } }).success,
    false
  );
  assert.equal(group.safeParse({ action: 'list' }).success, false);
});

test('the CLI still exposes snapshot and screenshot', () => {
  const help = spawnSync(process.execPath, [bin, '/cli', '--help'], {
    encoding: 'utf8',
  });
  for (const name of ['snapshot', 'screenshot'])
    assert.match(help.stdout + help.stderr, new RegExp(`^\\s+${name}\\b`, 'm'));
});

test('the CLI schema --command list is the full registry by design: every preset list is a subset of it, and the CLI-only tools stay', async () => {
  const cli = new Set(spec.commands.map(row => row.name));
  assert.deepEqual([...cli], Object.keys(chromeCommands));
  for (const name of ['snapshot', 'screenshot']) assert.ok(cli.has(name), name);
  for (const preset of Object.keys(presets)) {
    const surface = await tools(preset);
    if (!surface.tools.some(tool => tool.name === 'schema')) continue;
    for (const name of shape(surface, 'schema').properties.command.enum)
      assert.ok(cli.has(name), `${preset} lists ${name}, absent from the CLI`);
  }
});
