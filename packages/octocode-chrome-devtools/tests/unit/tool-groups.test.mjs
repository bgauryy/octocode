import test from 'node:test';
import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import {
  connectStdio,
  presets,
  toolGroups,
  withTools,
} from '../../dist/runtime.js';

const bin = resolve(
  import.meta.dirname,
  '../../bin/octocode-chrome-devtools.mjs'
);
const names = spec => spec.commands.map(command => command.name);
const research = names(presets.research);
const fullOnly = names(presets.full).filter(name => !research.includes(name));

test('groups cover every full-only tool exactly once', () => {
  const listed = Object.values(toolGroups).flat();
  assert.equal(new Set(listed).size, listed.length);
  assert.deepEqual([...listed].sort(), [...fullOnly].sort());
});

test('research + a group is research plus that group, in registry order', () => {
  const got = names(withTools('research', ['data']));
  assert.deepEqual([...got].sort(), [...research, ...toolGroups.data].sort());
  assert.deepEqual(
    names(withTools('research', Object.keys(toolGroups))),
    names(presets.full)
  );
});

test('a tool name adds that tool; a tool already in the preset is a no-op', () => {
  assert.deepEqual(
    names(withTools('research', ['proxy'])).filter(n => !research.includes(n)),
    ['proxy']
  );
  assert.equal(withTools('research', ['run', 'query']), presets.research);
  assert.equal(withTools('full', ['data']), presets.full);
});

test('an unknown name fails fast with the valid list', () => {
  assert.throws(
    () => withTools('research', ['nope']),
    /Unknown --tools name "nope".*data.*devtools.*capture.*cookies/
  );
  const run = spawnSync(process.execPath, [bin, '--mcp', '--tools', 'nope'], {
    encoding: 'utf8',
  });
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /Groups: data, devtools, capture/);
});

test('instructions and schema.command follow the resulting tool set', async () => {
  const spec = withTools('research', ['data']);
  for (const absent of ['dev-check', 'helper', 'proxy', 'check'])
    assert(!spec.instructions.includes(`\`${absent}\``), absent);
  const client = await connectStdio({
    command: process.execPath,
    args: [bin, '--mcp', '--preset', 'research', '--tools', 'data'],
  });
  try {
    const { tools } = await client.listTools();
    assert.deepEqual(tools.map(t => t.name).sort(), names(spec).sort());
    const schema = tools.find(t => t.name === 'schema');
    assert.deepEqual(
      [...schema.inputSchema.properties.command.enum].sort(),
      names(spec).sort()
    );
  } finally {
    await client.close();
  }
});
