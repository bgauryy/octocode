import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { afterEach, beforeEach, test } from 'vitest';
import { Type } from 'typebox';
import type { ToolDefinition } from '../src/types.js';
import { registerSkillTool } from '../src/tools/skill-tool.js';
import {
  setSkillGeneratorForTests,
  parseGeneratedSkill,
  assessSkillTriviality,
  type GeneratedSkill,
} from '../src/tools/call-skill.js';

let home: string;
let prev: string | undefined;
beforeEach(() => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), 'callskill-orch-'));
  prev = process.env.OCTOCODE_DYNAMIC_SKILLS_DIR;
  process.env.OCTOCODE_DYNAMIC_SKILLS_DIR = home;
});
afterEach(() => {
  setSkillGeneratorForTests(null);
  if (prev === undefined) delete process.env.OCTOCODE_DYNAMIC_SKILLS_DIR;
  else process.env.OCTOCODE_DYNAMIC_SKILLS_DIR = prev;
  fs.rmSync(home, { recursive: true, force: true });
});

function loadTool(): ToolDefinition {
  const tools = new Map<string, ToolDefinition>();
  const pi = { registerTool: (def: ToolDefinition) => tools.set(def.name, def) };
  registerSkillTool(pi, Type, new Set<string>(), (p, n, def) => {
    n.add(def.name);
    p.registerTool?.(def);
  }, () => []);
  return tools.get('skill')!;
}

const skill: GeneratedSkill = {
  name: 'release-checklist',
  description: 'Run the release checklist. Use before publishing.',
  reason: 'recurring multi-step release workflow',
  skillMd: `---\nname: release-checklist\ndescription: Run the release checklist. Use before publishing.\n---\n\n# Release Checklist\n\n## Steps\n1. Test.\n2. Bump.\n3. Publish.`,
};
const createMeta = { intent: 'run tests then bump version then publish the package', reason: 'recurring release workflow' };

async function run(tool: ToolDefinition, params: Record<string, unknown>) {
  const metadata = (params['metadata'] ?? {}) as Record<string, unknown>;
  const query = {
    reasoning: 'exercise the dynamic skill lifecycle contract',
    type: 'call',
    skillType: params['skillType'],
    mode: params['mode'],
    intent: metadata['intent'],
    reason: metadata['reason'],
    approveCreate: metadata['_approveCreate'],
    force: metadata['_force'],
  };
  return (await tool.execute('id', { queries: [query] })) as {
    content: Array<{ text: string }>;
    isError?: boolean;
    details: { status: string; skillName?: string; skillMd?: string };
  };
}

test('registers unified skill call operations with the documented schema', () => {
  const tool = loadTool();
  assert.equal(tool.name, 'skill');
  const props = (tool.parameters as {
    properties: { queries: { items: { properties: Record<string, unknown> } } };
  }).properties;
    assert.deepEqual(Object.keys(props), ['queries', 'queryRunType']);
  const queryProps = props.queries.items.properties;
  assert.ok(queryProps.type && queryProps.skillType && queryProps.intent && queryProps.mode);
});

test('auto miss PROPOSES creation instead of authoring', async () => {
  let gens = 0;
  setSkillGeneratorForTests(async () => {
    gens++;
    return skill;
  });
  const tool = loadTool();
  const res = await run(tool, { skillType: 'release-checklist', metadata: createMeta });
  assert.equal(res.details.status, 'proposal');
  assert.equal(gens, 0);
});

test('create authors, validates, registers, and points at SKILL.md', async () => {
  setSkillGeneratorForTests(async () => skill);
  const tool = loadTool();
  const res = await run(tool, { skillType: 'release-checklist', mode: 'create', metadata: createMeta });
  assert.equal(res.details.status, 'created');
  assert.ok(res.details.skillMd && fs.existsSync(res.details.skillMd));
});

test('create requires a reason', async () => {
  setSkillGeneratorForTests(async () => skill);
  const tool = loadTool();
  const res = await run(tool, { skillType: 'release-checklist', mode: 'create', metadata: { intent: createMeta.intent } });
  assert.equal(res.details.status, 'error');
  assert.match(res.content[0].text, /requires metadata.reason/i);
});

test('_approveCreate approves creation in auto mode', async () => {
  setSkillGeneratorForTests(async () => skill);
  const tool = loadTool();
  const res = await run(tool, { skillType: 'release-checklist', metadata: { ...createMeta, _approveCreate: true } });
  assert.equal(res.details.status, 'created');
});

test('auto reuses an existing skill without regenerating', async () => {
  let gens = 0;
  setSkillGeneratorForTests(async () => {
    gens++;
    return skill;
  });
  const tool = loadTool();
  await run(tool, { skillType: 'release-checklist', mode: 'create', metadata: createMeta });
  const res = await run(tool, { skillType: 'release-checklist', metadata: {} });
  assert.equal(res.details.status, 'reuse');
  assert.equal(gens, 1);
});

test('triviality guard declines a single-action request', async () => {
  setSkillGeneratorForTests(async () => skill);
  const tool = loadTool();
  const res = await run(tool, { skillType: 'greet', mode: 'create', metadata: { intent: 'say hi', reason: 'x' } });
  assert.equal(res.details.status, 'declined');
});

test('_force overrides the triviality decline', async () => {
    setSkillGeneratorForTests(async () => ({ ...skill, name: 'greet-flow', skillMd: skill.skillMd.replace('name: release-checklist', 'name: greet-flow') }));
  const tool = loadTool();
  const res = await run(tool, { skillType: 'greet-flow', mode: 'create', metadata: { intent: 'say hi', reason: 'x', _force: true } });
  assert.equal(res.details.status, 'created');
});

test('use mode errors on a miss', async () => {
  const tool = loadTool();
  const res = await run(tool, { skillType: 'ghost', mode: 'use' });
  assert.equal(res.details.status, 'error');
});

test('enhance requires an existing skill', async () => {
  setSkillGeneratorForTests(async () => skill);
  const tool = loadTool();
  const res = await run(tool, { skillType: 'ghost-flow', mode: 'enhance', metadata: { reason: 'x' } });
  assert.equal(res.details.status, 'error');
});

test('a generation failure surfaces as an error', async () => {
  setSkillGeneratorForTests(async () => {
    throw new Error('smith exploded');
  });
  const tool = loadTool();
  const res = await run(tool, { skillType: 'some-flow', mode: 'create', metadata: { intent: 'do a then b then c workflow', reason: 'x' } });
  assert.equal(res.details.status, 'error');
  assert.match(res.content[0].text, /generation failed/i);
});

test('an invalid generated skill is rejected by the validation gate', async () => {
  setSkillGeneratorForTests(async () => ({ ...skill, name: 'bad-flow', skillMd: 'no frontmatter here' }));
  const tool = loadTool();
  const res = await run(tool, { skillType: 'bad-flow', mode: 'create', metadata: { intent: 'a then b then c workflow', reason: 'x' } });
  assert.equal(res.details.status, 'error');
  assert.match(res.content[0].text, /validation gate/i);
});

test('list and delete CRUD', async () => {
  setSkillGeneratorForTests(async () => skill);
  const tool = loadTool();
  await run(tool, { skillType: 'release-checklist', mode: 'create', metadata: createMeta });
  const listed = await run(tool, { skillType: '', mode: 'list' });
  assert.equal(listed.details.status, 'listed');
  const del = await run(tool, { skillType: 'release-checklist', mode: 'delete' });
  assert.equal(del.details.status, 'deleted');
});

test('parseGeneratedSkill parses sentinel output and throws on missing SKILL_MD', () => {
  const out = ['===MANIFEST===', '{"name":"x-flow","description":"d","reason":"r"}', '===SKILL_MD===', '---\nname: x-flow\ndescription: d\n---\n# T\n## Steps\n1. a', '===END==='].join('\n');
  const g = parseGeneratedSkill(out, 'fallback');
  assert.equal(g.name, 'x-flow');
  assert.throws(() => parseGeneratedSkill('===MANIFEST===\n{}\n===SKILL_MD===\n===END===', 'x'));
});

test('assessSkillTriviality flags single actions and passes multi-step workflows', () => {
  assert.equal(assessSkillTriviality('greet', 'say hi').trivial, true);
  assert.equal(assessSkillTriviality('release', 'run tests then bump version then publish').trivial, false);
});
