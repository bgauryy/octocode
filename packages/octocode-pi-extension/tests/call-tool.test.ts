import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { afterEach, beforeEach, test } from 'vitest';
import { Type } from 'typebox';
import type { PiContext, ToolDefinition } from '../src/types.js';
import {
  registerCallTool,
  setToolGeneratorForTests,
  parseGeneratedTool,
  assessTriviality,
  type GeneratedTool,
} from '../src/tools/call-tool.js';

let home: string;
let prevHome: string | undefined;

beforeEach(() => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), 'calltool-orch-'));
  prevHome = process.env.OCTOCODE_HOME;
  process.env.OCTOCODE_HOME = home;
});
afterEach(() => {
  setToolGeneratorForTests(null);
  if (prevHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = prevHome;
  fs.rmSync(home, { recursive: true, force: true });
});

function loadTool(): ToolDefinition {
  const tools = new Map<string, ToolDefinition>();
  const pi = { registerTool: (def: ToolDefinition) => tools.set(def.name, def) };
  const names = new Set<string>();
  registerCallTool(pi, Type, names, (p, n, def) => {
    n.add(def.name);
    p.registerTool?.(def);
  });
  return tools.get('callTool')!;
}

const timeTool: GeneratedTool = {
  name: 'parseDuration',
  description: 'Parse a duration string to ms',
  keywords: ['duration', 'parse', 'ms'],
  capabilities: [],
  reason: 'reusable parsing not covered by a shell one-liner',
  source: `export default async ({ tz = 'UTC' } = {}) => ({ tz });`,
  test: `import fn from './tool.mjs';\nconst r = await fn({});\nif (r.tz !== 'UTC') process.exit(1);\nprocess.exit(0);`,
};

// A create call requires approval (mode:create) + a reason, and must not be a
// triviality-declined name.
const createMeta = { intent: 'parse a duration string', reason: 'reusable, non-trivial' };

async function run(tool: ToolDefinition, params: Record<string, unknown>, ctx?: PiContext) {
  const envelope = Array.isArray(params['queries'])
    ? params
    : { queries: [{ reasoning: 'exercise dynamic tool behavior', ...params }] };
  const res = (await tool.execute('id', envelope, undefined, undefined, ctx)) as {
    content: Array<{ text: string }>;
    isError?: boolean;
    details: { status: string; result?: unknown; toolName?: string };
  };
  return res;
}

test('registerCallTool registers a callTool with the documented schema', () => {
  const tool = loadTool();
  assert.equal(tool.name, 'callTool');
  const schema = tool.parameters as {
    properties: { queries?: { items?: { properties?: Record<string, unknown>; required?: string[] } } };
    required?: string[];
  };
    assert.deepEqual(Object.keys(schema.properties), ['queries', 'queryRunType']);
  assert.ok(schema.required?.includes('queries'));
  assert.ok(schema.properties.queries?.items?.properties?.['reasoning']);
  assert.ok(schema.properties.queries?.items?.required?.includes('reasoning'));
  assert.ok(schema.properties.queries?.items?.properties?.['toolType']);
});

test('callTool executes multiple validated operations in source order', async () => {
  const tool = loadTool();
  const res = await run(tool, {
    queries: [
      { reasoning: 'list dynamic tools first', toolType: 'inventory', mode: 'list' },
      { reasoning: 'list dynamic tools second', toolType: 'inventory', mode: 'list' },
    ],
  });
  assert.match(res.content[0]!.text, /2 queries succeeded/);
  assert.equal((res.details as unknown as { results: unknown[] }).results.length, 2);
});

test('callTool preflights a whole batch before an earlier delete', async () => {
  setToolGeneratorForTests(async () => timeTool);
  const tool = loadTool();
  await run(tool, { toolType: 'parseDuration', mode: 'create', metadata: createMeta });

  await assert.rejects(run(tool, {
    queries: [
      { reasoning: 'delete existing dynamic tool', toolType: 'parseDuration', mode: 'delete' },
      { reasoning: 'invalid creation without rationale', toolType: 'anotherTool', mode: 'create', metadata: { intent: 'x' } },
    ],
  }), /queries\[1\] failed preflight/);

  const listed = await run(tool, { toolType: 'inventory', mode: 'list' });
  assert.match(listed.content[0]!.text, /parseDuration/);
});

test('auto mode on a miss PROPOSES creation instead of silently generating', async () => {
  let gens = 0;
  setToolGeneratorForTests(async () => {
    gens++;
    return timeTool;
  });
  const tool = loadTool();
  const res = await run(tool, { toolType: 'parseDuration', metadata: { intent: 'x' } });
  assert.equal(res.details.status, 'proposal');
  assert.equal(gens, 0);
});

test('create mode generates, verifies, registers, and runs', async () => {
  setToolGeneratorForTests(async () => timeTool);
  const tool = loadTool();
  const res = await run(tool, { toolType: 'parseDuration', mode: 'create', metadata: createMeta });
  assert.equal(res.details.status, 'created-and-ran');
  assert.deepEqual(res.details.result, { tz: 'UTC' });
});

test('create without a reason is rejected', async () => {
  setToolGeneratorForTests(async () => timeTool);
  const tool = loadTool();
  const res = await run(tool, { toolType: 'parseDuration', mode: 'create', metadata: { intent: 'x' } });
  assert.equal(res.details.status, 'error');
  assert.match(res.content[0].text, /requires metadata.reason/i);
});

test('_approveCreate approves creation within auto mode', async () => {
  setToolGeneratorForTests(async () => timeTool);
  const tool = loadTool();
  const res = await run(tool, { toolType: 'parseDuration', metadata: { ...createMeta, _approveCreate: true } });
  assert.equal(res.details.status, 'created-and-ran');
});

test('auto mode reuses an existing tool without regenerating', async () => {
  let gens = 0;
  setToolGeneratorForTests(async () => {
    gens++;
    return timeTool;
  });
  const tool = loadTool();
  await run(tool, { toolType: 'parseDuration', mode: 'create', metadata: createMeta });
  const second = await run(tool, { toolType: 'parseDuration', metadata: { tz: 'UTC' } });
  assert.equal(gens, 1);
  assert.equal(second.details.status, 'ran');
});

test('triviality guard declines a capability a shell command already covers', async () => {
  setToolGeneratorForTests(async () => ({ ...timeTool, name: 'getCurrentTime' }));
  const tool = loadTool();
  const res = await run(tool, { toolType: 'getCurrentTime', mode: 'create', metadata: { intent: 'current time now', reason: 'x' } });
  assert.equal(res.details.status, 'declined');
  assert.match(res.content[0].text, /simple command/i);
});

test('_force overrides the triviality decline', async () => {
  setToolGeneratorForTests(async () => ({ ...timeTool, name: 'getCurrentTime' }));
  const tool = loadTool();
  const res = await run(tool, { toolType: 'getCurrentTime', mode: 'create', metadata: { intent: 'current time now', reason: 'x', _force: true } });
  assert.equal(res.details.status, 'created-and-ran');
});

test('list mode returns the inventory; delete removes a tool', async () => {
  setToolGeneratorForTests(async () => timeTool);
  const tool = loadTool();
  await run(tool, { toolType: 'parseDuration', mode: 'create', metadata: createMeta });
  const listed = await run(tool, { toolType: '', mode: 'list' });
  assert.equal(listed.details.status, 'listed');
  const deleted = await run(tool, { toolType: 'parseDuration', mode: 'delete' });
  assert.equal(deleted.details.status, 'deleted');
  const missing = await run(tool, { toolType: 'parseDuration', mode: 'delete' });
  assert.equal(missing.details.status, 'error');
});

test('assessTriviality flags trivial capabilities and passes non-trivial ones', () => {
  assert.equal(assessTriviality('getCurrentTime', 'current time').trivial, true);
  assert.equal(assessTriviality('toBase64', 'encode base64').trivial, true);
  assert.equal(assessTriviality('parseCronExpression', 'parse a cron schedule').trivial, false);
  // Regression: whole-word tokens must not substring-match legitimate names.
  assert.equal(assessTriviality('updateData', 'update a database record').trivial, false);
  assert.equal(assessTriviality('getHostnameParts', 'split a URL into labels').trivial, true); // 'hostname' token
  assert.equal(assessTriviality('toBase64', 'encode base64').trivial, true); // digit run kept intact
});

test('run mode errors on a miss instead of creating', async () => {
  setToolGeneratorForTests(async () => timeTool);
  const tool = loadTool();
  const res = await run(tool, { toolType: 'missingThing', mode: 'run' });
  assert.equal(res.isError, true);
  assert.equal(res.details.status, 'error');
});

test('enhance mode requires an existing tool', async () => {
  setToolGeneratorForTests(async () => timeTool);
  const tool = loadTool();
  const res = await run(tool, { toolType: 'nopeThing', mode: 'enhance', metadata: { reason: 'x' } });
  assert.equal(res.details.status, 'error');
});

test('a tool needing capabilities is blocked until approved via _allow', async () => {
  const netTool: GeneratedTool = {
    name: 'fetchThing',
    description: 'fetch',
    keywords: ['fetch'],
    capabilities: ['net'],
    reason: 'needs network access, not a shell one-liner',
    source: `export default async () => ({ ok: true });`,
    test: `import fn from './tool.mjs';\nawait fn();\nprocess.exit(0);`,
  };
  setToolGeneratorForTests(async () => netTool);
  const tool = loadTool();
  // create it first (approved), then run: first run without _allow is blocked.
  await run(tool, { toolType: 'fetchThing', mode: 'create', metadata: { intent: 'fetch', reason: 'net' } });
  const blocked = await run(tool, { toolType: 'fetchThing', metadata: {} });
  assert.equal(blocked.details.status, 'blocked');
  const approved = await run(tool, { toolType: 'fetchThing', metadata: { _allow: ['net'] } });
  assert.equal(approved.details.status, 'ran');
});

test('metadata._sandboxed:false requires explicit interactive approval and fails closed without UI', async () => {
  setToolGeneratorForTests(async () => timeTool);
  const tool = loadTool();
  const blocked = await run(tool, { toolType: 'parseDuration', mode: 'create', metadata: { ...createMeta, _sandboxed: false } });
  assert.equal(blocked.details.status, 'blocked');
  assert.match(blocked.content[0].text, /Non-sandboxed dynamic tool creation requires explicit user approval/i);
});

test('metadata._sandboxed:false proceeds only after user approval', async () => {
  setToolGeneratorForTests(async () => timeTool);
  const tool = loadTool();
  const prompts: string[] = [];
  const ctx = {
    hasUI: true,
    ui: {
      select: async (prompt: string) => {
        prompts.push(prompt);
        return 'Yes (run once)';
      },
    },
  } as unknown as PiContext;
  const res = await run(tool, { toolType: 'parseDuration', mode: 'create', metadata: { ...createMeta, _sandboxed: false } }, ctx);
  assert.equal(res.details.status, 'created-and-ran');
  assert.match(prompts[0]!, /Create non-sandboxed dynamic tool/);
});

test('a generator failure surfaces as an error outcome', async () => {
  setToolGeneratorForTests(async () => {
    throw new Error('smith exploded');
  });
  const tool = loadTool();
  const res = await run(tool, { toolType: 'parseThing', mode: 'create', metadata: { intent: 'x', reason: 'x' } });
  assert.equal(res.details.status, 'error');
  assert.match(res.content[0].text, /generation failed/i);
});

test('a generated tool failing its verification test is rejected', async () => {
  setToolGeneratorForTests(async () => ({
    ...timeTool,
    name: 'badTool',
    test: `process.exit(1);`,
  }));
  const tool = loadTool();
  const res = await run(tool, { toolType: 'badTool', mode: 'create', metadata: { intent: 'x', reason: 'x' } });
  assert.equal(res.details.status, 'error');
  assert.match(res.content[0].text, /verification gate/i);
});

// ─── parseGeneratedTool ─────────────────────────────────────────────

test('parseGeneratedTool parses sentinel-delimited worker output', () => {
  const output = [
    '===MANIFEST===',
    '{"name":"toSlug","description":"slugify","keywords":["slug"],"capabilities":[]}',
    '===SOURCE===',
    '```js',
    `export default async ({ s }) => ({ slug: s.toLowerCase() });`,
    '```',
    '===TEST===',
    `import fn from './tool.mjs'; process.exit(0);`,
    '===END===',
  ].join('\n');
  const g = parseGeneratedTool(output, 'fallbackName');
  assert.equal(g.name, 'toSlug');
  assert.ok(g.source.startsWith('export default'));
  assert.equal(g.capabilities.length, 0);
});

test('parseGeneratedTool throws on missing sections', () => {
  assert.throws(() => parseGeneratedTool('===MANIFEST===\n{}\n===SOURCE===\n===TEST===\n===END===', 'x'));
});

test('parseGeneratedTool throws on invalid manifest JSON', () => {
  const output = '===MANIFEST===\n{bad\n===SOURCE===\ncode\n===TEST===\ntest\n===END===';
  assert.throws(() => parseGeneratedTool(output, 'x'));
});

test('parseGeneratedTool falls back to the provided name and filters capabilities', () => {
  const output =
    '===MANIFEST===\n{"capabilities":["net","bogus"]}\n===SOURCE===\ncode\n===TEST===\ntest\n===END===';
  const g = parseGeneratedTool(output, 'fallbackName');
  assert.equal(g.name, 'fallbackName');
  assert.deepEqual(g.capabilities, ['net']);
});
