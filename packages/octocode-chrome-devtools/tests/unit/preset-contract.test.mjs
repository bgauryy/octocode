import test from 'node:test';
import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import {
  connectStdio,
  presets,
  spec,
  annotationRules,
} from '../../dist/runtime.js';
import { chromeCommands } from '../../dist/engine/chrome-contract.mjs';

const bin = resolve(
  import.meta.dirname,
  '../../bin/octocode-chrome-devtools.mjs'
);
const HEADING = 'Permission and sensitivity:';
const REFERENCE = 'https://chromedevtools.github.io/devtools-protocol/';
// Plain words that are also tool names; they are checked in backticks only.
const GENERIC = new Set(['run', 'open', 'query', 'check', 'schema', 'skill']);

for (const [name, preset] of Object.entries(presets)) {
  const names = preset.commands.map(command => command.name);
  const text = preset.instructions;
  // The plan-op cheat sheet names plan operations, which share names with tools.
  const body = text
    .slice(0, text.indexOf(HEADING))
    .split('\n')
    .filter(line => !line.startsWith('Plan ops:'))
    .join('\n');
  test(`${name}: instructions name only tools this preset exports`, () => {
    for (const absent of Object.keys(chromeCommands).filter(
      tool => !names.includes(tool)
    )) {
      assert(!body.includes(`\`${absent}\``), absent);
      if (!GENERIC.has(absent))
        assert.doesNotMatch(body, new RegExp(`(?<![\\w-])${absent}(?![\\w-])`));
    }
  });
  test(`${name}: instructions always carry the permission section`, () => {
    const section = text.slice(text.indexOf(HEADING));
    assert(section.length > HEADING.length);
    for (const phrase of [
      'untrusted data',
      'Ask the user before',
      'real or logged-in profile',
      'login walls, CAPTCHA or MFA',
      'allowOrigins',
    ])
      assert(section.includes(phrase), phrase);
  });
  test(`${name}: the CDP reference appears only with cdp and protocol`, () => {
    assert.equal(
      text.includes(REFERENCE),
      names.includes('cdp') && names.includes('protocol')
    );
  });
  test(`${name}: the removed step tool is absent from tools/list`, async () => {
    const client = await connectStdio({
      command: process.execPath,
      args: [bin, '--mcp', '--preset', name],
    });
    try {
      const { tools } = await client.listTools();
      assert.deepEqual(
        tools.map(tool => tool.name),
        names
      );
      assert(!tools.some(tool => tool.name === 'step'));
    } finally {
      await client.close();
    }
  });
}

test('default preset holds cdp, protocol and group; slim holds none; only full holds helper', () => {
  const has = (preset, tool) =>
    preset.commands.some(command => command.name === tool);
  for (const tool of ['cdp', 'protocol', 'group']) {
    assert(has(presets.research, tool));
    assert(has(presets.full, tool));
    assert(!has(presets.slim, tool));
  }
  // Plan ops and the page-snapshot and page-screenshot recipes capture the same evidence, so the standalone tools are /cli only.
  for (const tool of ['snapshot', 'screenshot'])
    for (const preset of Object.values(presets))
      assert(!has(preset, tool), tool);
  assert(has(presets.research, 'run') && has(presets.slim, 'run'));
  assert(has(presets.full, 'helper'));
  assert(!has(presets.research, 'helper'));
  assert(!has(presets.slim, 'helper'));
});

test('the step command is gone from /cli', () => {
  const help = spawnSync(process.execPath, [bin, '/cli', '--help'], {
    encoding: 'utf8',
  });
  assert(!/^\s+step\b/m.test(help.stdout + help.stderr));
  assert(!Object.hasOwn(chromeCommands, 'step'));
});

test('the annotation table classifies every tool; read-only tools are idempotent, not destructive', () => {
  for (const { name, annotations } of presets.full.commands) {
    assert(Object.hasOwn(annotationRules, name), `${name} unclassified`);
    assert.equal(typeof annotations.readOnlyHint, 'boolean');
    if (annotations.readOnlyHint) {
      assert.equal(annotations.idempotentHint, true, name);
      assert.equal(annotations.destructiveHint, false, name);
    }
    assert(!(annotations.readOnlyHint && annotations.destructiveHint), name);
  }
  // helper reaches prune, api-replay and har-redact; group opens and closes tabs.
  for (const name of ['helper', 'group'])
    assert.equal(annotationRules[name].kind, 'destructive', name);
  assert.deepEqual(
    Object.keys(annotationRules).toSorted(),
    spec.commands.map(command => command.name).toSorted()
  );
});
