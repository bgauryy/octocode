import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { afterEach, test } from 'vitest';
import {
  discoverSkillStates,
  discoverSkills,
  formatSkillUsageLines,
  getSkillUsage,
  recordSkillLoad,
  registerSkillTool,
  resetSkillUsageForTests,
} from '../src/tools/skill-tool.js';
import { setSkillEnabled } from '@octocodeai/octocode-awareness/mcp-state';
import { openOctocodeDb } from '../src/tools/storage-policy.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { renderAvailableSkillsAddendum } from '../src/tools/skill-catalog.js';
import type { ToolDefinition, ToolCallResult, PiContext, SkillInfo } from '../src/types.js';

afterEach(() => {
  resetSkillUsageForTests();
});

function makeSkillDir(root: string, name: string, description: string, extraFiles: Record<string, string> = {}): string {
  const dir = path.join(root, name);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'SKILL.md'), `---\nname: ${name}\ndescription: "${description}"\n---\n\n# ${name}\n\nDo the workflow.\n`);
  for (const [rel, content] of Object.entries(extraFiles)) {
    const filePath = path.join(dir, rel);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, content);
  }
  return dir;
}

function tmpWorkspace(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'octo-skill-tool-'));
}

// ─── discovery ─────────────────────────────────────────────────────────────────────

test('discoverSkills finds project skills under .agents/skills with frontmatter name + description', () => {
  const cwd = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'demo-flow', 'Demo workflow skill.');
  const skills = discoverSkills(cwd);
  const demo = skills.find((s) => s.name === 'demo-flow');
  assert.ok(demo, 'project skill discovered');
  assert.equal(demo!.description, 'Demo workflow skill.');
  assert.equal(demo!.source, 'project');
  assert.ok(demo!.path.endsWith('SKILL.md'));
  assert.equal(demo!.dir, path.dirname(demo!.path));
});

test('skill overrides keep disabled skills in settings inventory but remove them from the agent surface', () => {
  const cwd = tmpWorkspace();
  const previousHome = process.env['OCTOCODE_HOME'];
  const previousMode = process.env['OCTOCODE_STORAGE_MODE'];
  process.env['OCTOCODE_HOME'] = path.join(tmpWorkspace(), 'home');
  process.env['OCTOCODE_STORAGE_MODE'] = 'persistent';
  try {
    makeSkillDir(path.join(cwd, '.agents', 'skills'), 'demo-flow', 'Demo workflow skill.');
    setSkillEnabled(openOctocodeDb(), path.resolve(cwd), 'demo-flow', false);
    assert.equal(discoverSkills(cwd).some((skill) => skill.name === 'demo-flow'), false);
    assert.doesNotMatch(renderAvailableSkillsAddendum(discoverSkills(cwd)), /demo-flow/);
    const state = discoverSkillStates(cwd).find((skill) => skill.name === 'demo-flow');
    assert.equal(state?.enabled, false);
    assert.equal(state?.path.endsWith('SKILL.md'), true);
  } finally {
    if (previousHome === undefined) delete process.env['OCTOCODE_HOME'];
    else process.env['OCTOCODE_HOME'] = previousHome;
    if (previousMode === undefined) delete process.env['OCTOCODE_STORAGE_MODE'];
    else process.env['OCTOCODE_STORAGE_MODE'] = previousMode;
  }
});

test('discoverSkills: Pi-provided entries take precedence over disk scan for the same name', () => {
  const cwd = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'demo-flow', 'Disk description.');
  const piSkills: SkillInfo[] = [{ name: 'demo-flow', description: 'Pi description.', path: '/pi/demo-flow/SKILL.md', source: 'user', scope: 'global' }];
  const skills = discoverSkills(cwd, piSkills);
  const demo = skills.find((s) => s.name === 'demo-flow');
  assert.equal(demo!.description, 'Pi description.', 'Pi is the live session authority');
  assert.equal(demo!.source, 'user/global');
});

test('discoverSkills resolves Pi prompt metadata without a path to the loadable disk skill', () => {
  const cwd = tmpWorkspace();
  const dir = makeSkillDir(path.join(cwd, '.agents', 'skills'), 'demo-flow', 'Disk description.');
  const skills = discoverSkills(cwd, [{ name: 'demo-flow', description: 'Prompt-only metadata.' }]);
  const demo = skills.find((skill) => skill.name === 'demo-flow')!;

  assert.equal(demo.path, path.join(dir, 'SKILL.md'));
  assert.equal(demo.source, 'pi');
  assert.equal(demo.description, 'Prompt-only metadata.');
});

test('discoverSkills scans the common ecosystem roots (claude/cursor/codex/octocode/pi) in both scopes', () => {
  const cwd = tmpWorkspace();
  const home = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.claude', 'skills'), 'claude-skill', 'From project claude.');
  makeSkillDir(path.join(cwd, '.cursor', 'skills'), 'cursor-skill', 'From project cursor.');
  makeSkillDir(path.join(cwd, '.codex', 'skills'), 'codex-skill', 'From project codex.');
  makeSkillDir(path.join(cwd, '.octocode', 'skills'), 'octo-skill', 'From project octocode.');
  makeSkillDir(path.join(cwd, '.pi', 'skills'), 'pi-skill', 'From project pi.');
  makeSkillDir(path.join(home, '.claude', 'skills'), 'home-claude-skill', 'From user claude.');
  makeSkillDir(path.join(home, '.agents', 'skills'), 'home-agents-skill', 'From user agents.');
  makeSkillDir(path.join(home, '.pi', 'agent', 'skills'), 'home-pi-skill', 'From user pi.');
  const bySource = Object.fromEntries(discoverSkills(cwd, undefined, home).map((s) => [s.name, s.source]));
  assert.equal(bySource['claude-skill'], 'project:claude');
  assert.equal(bySource['home-agents-skill'], 'user:agents');
  assert.equal(bySource['cursor-skill'], 'project:cursor');
  assert.equal(bySource['codex-skill'], 'project:codex');
  assert.equal(bySource['octo-skill'], 'project:octocode');
  assert.equal(bySource['pi-skill'], 'project:pi');
  assert.equal(bySource['home-claude-skill'], 'user:claude');
  assert.equal(bySource['home-pi-skill'], 'user', 'the primary pi user root keeps its plain label');
});

test('discoverSkills dedupes by NAME across roots — most-authoritative root wins', () => {
  const cwd = tmpWorkspace();
  const home = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'shared-skill', 'Canonical .agents version.');
  makeSkillDir(path.join(cwd, '.claude', 'skills'), 'shared-skill', 'Claude copy.');
  makeSkillDir(path.join(home, '.cursor', 'skills'), 'shared-skill', 'User cursor copy.');
  const skills = discoverSkills(cwd, undefined, home);
  const matches = skills.filter((s) => s.name === 'shared-skill');
  assert.equal(matches.length, 1, 'one entry per name');
  assert.equal(matches[0]!.description, 'Canonical .agents version.');
  assert.equal(matches[0]!.source, 'project');
});

test('discoverSkills rejects names that violate Agent Skills lowercase naming', () => {
  const cwd = tmpWorkspace();
  const home = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'Release-Check', 'Project version.');
  makeSkillDir(path.join(home, '.pi', 'skills'), 'release-check', 'User copy.');
  const matches = discoverSkills(cwd, undefined, home)
    .filter((skill) => skill.name.toLowerCase() === 'release-check');

  assert.equal(matches.length, 1);
  assert.equal(matches[0]!.description, 'User copy.');
});

test('discoverSkills skips directories without SKILL.md and missing roots without throwing', () => {
  const cwd = tmpWorkspace();
  fs.mkdirSync(path.join(cwd, '.agents', 'skills', 'not-a-skill'), { recursive: true });
  assert.doesNotThrow(() => discoverSkills(cwd));
  assert.ok(!discoverSkills(cwd).some((s) => s.name === 'not-a-skill'));
});

test('discoverSkills uses canonical containment, symlink, and size defenses', () => {
  const cwd = tmpWorkspace();
  const root = path.join(cwd, '.agents', 'skills');
  const outside = tmpWorkspace();
  makeSkillDir(outside, 'escaped-skill', 'Must not escape the source root.');
  fs.mkdirSync(root, { recursive: true });
  fs.symlinkSync(path.join(outside, 'escaped-skill'), path.join(root, 'escaped-skill'), 'dir');
  const oversized = makeSkillDir(root, 'oversized-skill', 'Too large.');
  fs.appendFileSync(path.join(oversized, 'SKILL.md'), 'x'.repeat(600_000));

  const names = discoverSkills(cwd).map((skill) => skill.name);
  assert.ok(!names.includes('escaped-skill'));
  assert.ok(!names.includes('oversized-skill'));
});

test('discoverSkills filters both Awareness aliases because coordination is prompt- and tool-owned in Pi', () => {
  const cwd = tmpWorkspace();
  const names = ['octocode-awareness', 'octocode-awareness'];
  for (const name of names) makeSkillDir(path.join(cwd, '.agents', 'skills'), name, 'External-agent skill copy.');
  const piSkills: SkillInfo[] = names.map((name) => ({ name, description: 'Pi copy.', path: `/pi/${name}/SKILL.md` }));
  const skills = discoverSkills(cwd, piSkills);
  for (const name of names) assert.ok(!skills.some((skill) => skill.name === name), `${name} is not loadable in Pi`);
});

// ─── usage ledger (observability) ──────────────────────────────────────────────────────────────

test('usage ledger records loads and formats dashboard lines, newest first', () => {
  recordSkillLoad('a-skill', 1000);
  recordSkillLoad('a-skill', 2000);
  recordSkillLoad('b-skill', 3000);
  assert.equal(getSkillUsage().get('a-skill')?.count, 2);
  const lines = formatSkillUsageLines();
  assert.deepEqual(lines, ['- b-skill: loaded 1×', '- a-skill: loaded 2×']);
});

// ─── the unified skill tool (queries[] envelope) ─────────────────────────────────────────

async function makeTool(piSkills?: SkillInfo[]): Promise<ToolDefinition> {
  const { Type } = await import('typebox');
  let def: ToolDefinition | undefined;
  registerSkillTool(
    { registerTool: (d: ToolDefinition) => { def = d; } },
    Type,
    new Set<string>(),
    (pi, names, d) => registerUniqueTool(pi, names, d),
    () => piSkills,
  );
  assert.ok(def);
  return def!;
}

async function run(def: ToolDefinition, params: Record<string, unknown>, cwd: string): Promise<ToolCallResult> {
  try {
    return await (def.execute('id', params, undefined, undefined, { cwd } as unknown as PiContext) as Promise<ToolCallResult>);
  } catch (err) {
    // executeQueryBatch throws QueryBatchError on validation/execution failures;
    // convert to an isError result so tests can assert on the text.
    const msg = err instanceof Error ? err.message : String(err);
    return { content: [{ type: 'text', text: msg }], isError: true };
  }
}

/** Convenience: wrap in the queries[] envelope. */
function q(items: Record<string, unknown>[]): Record<string, unknown> {
  return { queries: items };
}

// ─── type:load ────────────────────────────────────────────────────────────────────────

test('queries must be a non-empty array — top-level validation', async () => {
  const cwd = tmpWorkspace();
  const def = await makeTool();
  const r1 = await run(def, {}, cwd);
  assert.equal(r1.isError, true);
  assert.match((r1.content[0] as { text: string }).text, /queries must be a non-empty array/);
  const r2 = await run(def, { queries: [] }, cwd);
  assert.equal(r2.isError, true);
});

test('each query requires non-empty reasoning', async () => {
  const cwd = tmpWorkspace();
  const def = await makeTool();
  const r = await run(def, q([{ type: 'load', action: 'list' }]), cwd);
  assert.equal(r.isError, true);
  assert.match((r.content[0] as { text: string }).text, /reasoning/);
});

test('type:load action:load returns SKILL.md content, directory, and shipped files; records usage', async () => {
  const cwd = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'demo-flow', 'Demo.', { 'scripts/run.sh': '#!/bin/sh\n' });
  const def = await makeTool();
  const res = await run(def, q([{ reasoning: 'Need the demo workflow.', type: 'load', name: 'demo-flow', reason: 'The current task needs the demo workflow.' }]), cwd);
  const text = (res.content[0] as { text: string }).text;
  assert.equal(res.isError ?? false, false);
  assert.match(text, /skill: demo-flow \[project\]/);
  assert.match(text, /directory: /);
  assert.match(text, /Resolve every relative path/);
  assert.match(text, /files: scripts\/run\.sh/);
  assert.match(text, /# demo-flow/, 'full SKILL.md body returned');
  assert.equal(getSkillUsage().get('demo-flow')?.count, 1, 'load recorded for observability');
});

test('partial skill content and file lists recover completely through executable Octocode continuations', async () => {
  // The canonical local tools permit the user's home by default. Keep this
  // isolated fixture inside that boundary instead of weakening path guards.
  const cwd = fs.mkdtempSync(path.join(os.homedir(), '.octo-skill-recovery-'));
  try {
    const extras = Object.fromEntries(Array.from({ length: 65 }, (_, i) => [`references/part-${i}.md`, `Part ${i}`]));
    extras['references/deep/nested/last.md'] = 'Deep reference';
    extras['.hidden-note.md'] = 'Hidden reference';
    const dir = makeSkillDir(path.join(cwd, '.agents', 'skills'), 'large-flow', 'Large fixture.', extras);
    const skillPath = path.join(dir, 'SKILL.md');
    const original = fs.readFileSync(skillPath, 'utf8') + Array.from({ length: 2200 }, (_, i) => `Requirement ${i}: preserve café and 🧭.\n`).join('');
    fs.writeFileSync(skillPath, original);
    const def = await makeTool();
    const res = await run(def, q([{ reasoning: 'Load fixture.', name: 'large-flow', reason: 'Verify complete retrieval.' }]), cwd);
    const details = res.details as {
      isPartial: boolean; partialReasons: string[]; files: string[];
      content: { returnedChars: number };
      next: Record<string, { tool: string; query: { queries: Array<{ action: string; server: string; tool: string; arguments: { queries: Record<string, unknown>[] } }> } }>;
    };
    assert.equal(details.isPartial, true);
    assert.ok(details.partialReasons.includes('content-limit'));
    assert.ok(details.partialReasons.includes('file-limit'));
    assert.ok(details.partialReasons.includes('file-depth'));
    assert.ok(details.partialReasons.includes('file-filter'));
    const visible = res.content.flatMap((part) => part.type === 'text' ? [part.text] : []).join('\n');
    assert.ok(visible.includes(original.slice(0, details.content.returnedChars)), 'the entire first page survives the model-visible result budget');
    assert.ok(visible.includes('MCPTool') && visible.includes('charOffset'), 'recovery is visible to the model');
    const cli = fileURLToPath(new URL('../../octocode/out/octocode.js', import.meta.url));
    const execute = (tool: string, query: Record<string, unknown>) => {
      const child = spawnSync(process.execPath, [cli, 'tools', tool, '--queries', JSON.stringify(query), '--compact'], { cwd, encoding: 'utf8', timeout: 20_000, env: { ...process.env, ENABLE_LOCAL: 'true' } });
      assert.equal(child.status, 0, `${child.stdout}\n${child.stderr}`);
      const response = JSON.parse(child.stdout);
      const row = response.results[0];
      assert.notEqual(row.status, 'error', JSON.stringify(row));
      return { ...row.data, base: response.base ?? cwd };
    };
    const unwrap = (name: string) => {
      const next = details.next[name]!;
      assert.equal(next.tool, 'MCPTool');
      const call = next.query.queries[0]!;
      assert.equal(call.action, 'call');
      assert.equal(call.server, 'octocode');
      return { tool: call.tool, query: call.arguments.queries[0]! };
    };
    let contentNext: { tool: string; query: Record<string, unknown> } | undefined = unwrap('content');
    let recovered = original.slice(0, details.content.returnedChars);
    let pages = 0;
    while (contentNext) {
      assert.ok(++pages < 30, 'content recovery makes bounded progress');
      const data = execute(contentNext.tool, contentNext.query);
      recovered += data.content;
      contentNext = data.next?.continueChars;
      if (data.isPartial) assert.ok(contentNext, 'every partial read supplies its next call');
    }
    assert.equal(recovered, original, 'prefix and canonical continuation pages cover every instruction exactly');
    let filesNext: { tool: string; query: Record<string, unknown> } | undefined = unwrap('files');
    const recoveredFiles = new Set(details.files);
    pages = 0;
    while (filesNext) {
      assert.ok(++pages < 30, 'file recovery makes bounded progress');
      const data = execute(filesNext.tool, filesNext.query);
      for (const file of data.files ?? []) recoveredFiles.add(path.relative(dir, path.resolve(data.base, file.path)));
      filesNext = data.next?.nextPage ?? data.next?.expandLimit;
    }
    for (const name of Object.keys(extras)) assert.ok(recoveredFiles.has(name), `recovered ${name}`);
  } finally {
    fs.rmSync(cwd, { recursive: true, force: true });
  }
}, 30_000);

test('type:load is default when type field omitted — load action also default', async () => {
  const cwd = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'demo-flow', 'Demo.');
  const def = await makeTool();
  // No type field, no action field — should default to type:load action:load
  const res = await run(def, q([{ reasoning: 'Need the demo.', name: 'demo-flow', reason: 'The current task needs the demo workflow.' }]), cwd);
  assert.equal(res.isError ?? false, false);
  assert.match((res.content[0] as { text: string }).text, /skill: demo-flow/);
});

test('type:load action:load requires a user-visible trigger reason', async () => {
  const cwd = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'demo-flow', 'Demo.');
  const def = await makeTool();
  const res = await run(def, q([{ reasoning: 'Testing.', type: 'load', action: 'load', name: 'demo-flow' }]), cwd);
  assert.equal(res.isError, true);
  assert.match((res.content[0] as { text: string }).text, /requires reason explaining why it matches the current task/);
});

test('type:load action:load on unknown name errors with the available catalog', async () => {
  const cwd = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'demo-flow', 'Demo.');
  const def = await makeTool();
  const res = await run(def, q([{ reasoning: 'Testing.', type: 'load', name: 'nope', reason: 'The task needs a workflow.' }]), cwd);
  assert.equal(res.isError, true);
  assert.match((res.content[0] as { text: string }).text, /Unknown skill: nope/);
  assert.match((res.content[0] as { text: string }).text, /demo-flow/);
});

test('type:load action:list shows every discovered skill with source and session usage', async () => {
  const cwd = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'demo-flow', 'Demo workflow.');
  const def = await makeTool();
  await run(def, q([{ reasoning: 'Load first.', type: 'load', name: 'demo-flow', reason: 'The current task needs the demo workflow.' }]), cwd);
  const res = await run(def, q([{ reasoning: 'List all skills.', type: 'load', action: 'list' }]), cwd);
  const text = (res.content[0] as { text: string }).text;
  assert.match(text, /skill\(\{queries:/);
  assert.match(text, /- demo-flow \[project\] \(loaded 1× this session\): Demo workflow\./);
});

// ─── ordered multi-query ──────────────────────────────────────────────────────────────────

test('multi-query: two type:load items execute in order and both succeed', async () => {
  const cwd = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'skill-a', 'Skill A.');
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'skill-b', 'Skill B.');
  const def = await makeTool();
  const res = await run(def, q([
    { reasoning: 'Load skill-a.', type: 'load', name: 'skill-a', reason: 'Task needs skill A.' },
    { reasoning: 'Load skill-b.', type: 'load', name: 'skill-b', reason: 'Task needs skill B.' },
  ]), cwd);
  assert.equal(res.isError ?? false, false);
  const text = (res.content[0] as { text: string }).text;
  assert.match(text, /2 quer/);
  assert.equal(getSkillUsage().get('skill-a')?.count, 1);
  assert.equal(getSkillUsage().get('skill-b')?.count, 1);
});

test('multi-query: stops on first error and does not execute subsequent items', async () => {
  const cwd = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'skill-a', 'Skill A.');
  const def = await makeTool();
  const res = await run(def, q([
    { reasoning: 'Load unknown.', type: 'load', name: 'nope', reason: 'Task needs this.' },
    { reasoning: 'Load skill-a.', type: 'load', name: 'skill-a', reason: 'Task needs skill A.' },
  ]), cwd);
  assert.equal(res.isError, true);
  assert.match((res.content[0] as { text: string }).text, /queries\[0\] failed/);
  // skill-a was NOT loaded because execution stopped at index 0
  assert.equal(getSkillUsage().get('skill-a'), undefined);
});

// ─── type:call dynamic skill lifecycle ────────────────────────────────────────────────

/**
 * Isolate dynamic-skill filesystem operations to a temp dir by pointing
 * OCTOCODE_DYNAMIC_SKILLS_DIR at a throw-away directory.
 */
async function withTempSkillsDir<T>(fn: (dir: string) => Promise<T>): Promise<T> {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-dyn-skills-'));
  const old = process.env['OCTOCODE_DYNAMIC_SKILLS_DIR'];
  process.env['OCTOCODE_DYNAMIC_SKILLS_DIR'] = dir;
  try {
    return await fn(dir);
  } finally {
    if (old === undefined) delete process.env['OCTOCODE_DYNAMIC_SKILLS_DIR'];
    else process.env['OCTOCODE_DYNAMIC_SKILLS_DIR'] = old;
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

test('type:call mode:list returns dynamic skill list (empty initially)', async () => {
  const cwd = tmpWorkspace();
  const def = await makeTool();
  await withTempSkillsDir(async () => {
    const res = await run(def, q([{ reasoning: 'List dynamic skills.', type: 'call', skillType: 'any', mode: 'list' }]), cwd);
    assert.equal(res.isError ?? false, false);
    const text = (res.content[0] as { text: string }).text;
    assert.match(text, /\[SKILLS\]/);
  });
});

test('type:call mode:auto proposes creation when skill is missing (no silent authoring)', async () => {
  const cwd = tmpWorkspace();
  const def = await makeTool();
  await withTempSkillsDir(async () => {
    const res = await run(def, q([{
      reasoning: 'Need a multi-step deploy workflow.',
      type: 'call',
      skillType: 'deploy-workflow',
      mode: 'auto',
      intent: 'deploy the service step by step then run smoke tests and rollback on failure',
    }]), cwd);
    assert.equal(res.isError ?? false, false);
    const text = (res.content[0] as { text: string }).text;
    // Should be a PROPOSAL since no skill exists and approveCreate is not set
    assert.match(text, /\[PROPOSAL\]/);
  });
});

test('type:call mode:use on missing skill returns error (no creation in use mode)', async () => {
  const cwd = tmpWorkspace();
  const def = await makeTool();
  await withTempSkillsDir(async () => {
    const res = await run(def, q([{
      reasoning: 'Try to reuse existing skill only.',
      type: 'call',
      skillType: 'nonexistent-skill',
      mode: 'use',
    }]), cwd);
    assert.equal(res.isError, true);
    assert.match((res.content[0] as { text: string }).text, /\[ERROR\]/);
  });
});

test('type:call trivial single-action intent is declined unless force:true', async () => {
  const cwd = tmpWorkspace();
  const def = await makeTool();
  await withTempSkillsDir(async () => {
    // Short intent with no multi-step signal → declined
    const declined = await run(def, q([{
      reasoning: 'Creating a skill for a simple action.',
      type: 'call',
      skillType: 'run-test',
      mode: 'create',
      intent: 'run test',
      reason: 'Need it.',
      approveCreate: true,
    }]), cwd);
    assert.equal(declined.isError ?? false, false);
    assert.match((declined.content[0] as { text: string }).text, /\[DECLINED\]/);

    // Same but with force:true and a mock generator — generator injected via setSkillGeneratorForTests
    const { setSkillGeneratorForTests } = await import('../src/tools/call-skill.js');
    const mockSkillMd = '---\nname: run-test\ndescription: Runs tests then reports results and retries on failure.\n---\n\n# Run Test\n\n## Steps\n1. Run the test suite with npm test.\n2. Capture and display output.\n3. On failure retry once and report.\n';
    setSkillGeneratorForTests(async () => ({
      name: 'run-test',
      description: 'Runs tests then reports results and retries on failure.',
      reason: 'Recurring multi-step test workflow.',
      skillMd: mockSkillMd,
    }));
    try {
      const forced = await run(def, q([{
        reasoning: 'Creating a forced skill.',
        type: 'call',
        skillType: 'run-test',
        mode: 'create',
        intent: 'run test step by step then retry and report on failure',
        reason: 'This is a recurring multi-step test workflow.',
        approveCreate: true,
        force: true,
      }]), cwd);
      // Should succeed with the mock generator
      assert.equal(forced.isError ?? false, false);
      assert.match((forced.content[0] as { text: string }).text, /\[CREATED\]/);
    } finally {
      setSkillGeneratorForTests(null);
    }
  });
});

test('type:call mode:delete removes a dynamic skill', async () => {
  const cwd = tmpWorkspace();
  const def = await makeTool();
  const { setSkillGeneratorForTests } = await import('../src/tools/call-skill.js');
  await withTempSkillsDir(async () => {
    const mockSkillMd = '---\nname: cleanup-flow\ndescription: Cleans up build artifacts then archives logs and notifies the team.\n---\n\n# Cleanup Flow\n\n## Steps\n1. Remove build artifacts.\n2. Archive logs.\n3. Send team notification.\n';
    setSkillGeneratorForTests(async () => ({
      name: 'cleanup-flow',
      description: 'Cleans up build artifacts then archives logs and notifies the team.',
      reason: 'Recurring cleanup workflow.',
      skillMd: mockSkillMd,
    }));
    try {
      // First create the skill
      const created = await run(def, q([{
        reasoning: 'Create cleanup-flow skill.',
        type: 'call',
        skillType: 'cleanup-flow',
        mode: 'create',
        intent: 'clean up build artifacts then archive logs and notify the team',
        reason: 'Recurring cleanup workflow after builds.',
        approveCreate: true,
        force: true,
      }]), cwd);
      assert.match((created.content[0] as { text: string }).text, /\[CREATED\]/);

      // Now delete it
      const deleted = await run(def, q([{
        reasoning: 'Remove obsolete cleanup-flow skill.',
        type: 'call',
        skillType: 'cleanup-flow',
        mode: 'delete',
      }]), cwd);
      assert.equal(deleted.isError ?? false, false);
      assert.match((deleted.content[0] as { text: string }).text, /\[DELETED\]/);

      // Deleting again reports an error
      const gone = await run(def, q([{
        reasoning: 'Try to delete already-removed skill.',
        type: 'call',
        skillType: 'cleanup-flow',
        mode: 'delete',
      }]), cwd);
      assert.equal(gone.isError, true);
    } finally {
      setSkillGeneratorForTests(null);
    }
  });
});

test('type:call create requires explicit reason field (replaces metadata.reason)', async () => {
  const cwd = tmpWorkspace();
  const def = await makeTool();
  await withTempSkillsDir(async () => {
    const res = await run(def, q([{
      reasoning: 'Create without reason.',
      type: 'call',
      skillType: 'no-reason-workflow',
      mode: 'create',
      intent: 'step one then step two then step three',
      approveCreate: true,
      force: true,
    }]), cwd);
    // No reason provided → error from orchestrate
    assert.equal(res.isError, true);
    assert.match((res.content[0] as { text: string }).text, /\[ERROR\]/);
  });
});

// ─── render rows ────────────────────────────────────────────────────────────────────────

test('skill render rows: single type:load explains trigger, suppresses success, shows errors', async () => {
  const cwd = tmpWorkspace();
  makeSkillDir(path.join(cwd, '.agents', 'skills'), 'demo-flow', 'Demo.');
  const def = await makeTool();
  const theme = { fg: (_c: string, t: string) => t, bold: (t: string) => t };
  const reason = 'The task needs a repeatable demo workflow.';

  // renderCall: shows skill name and reason
  const callRow = def.renderCall!(q([{ reasoning: 'Load demo-flow.', type: 'load', name: 'demo-flow', reason }]), theme).render(100).join('\n');
  assert.match(callRow, /◆ skill · demo-flow/);
  assert.match(callRow, /\n  The task needs a repeatable demo workflow\./);
  assert.doesNotMatch(callRow, /why:|reasoning:/);

  // Success: renderResult returns no rows (silent on success)
  const ok = await run(def, q([{ reasoning: 'Load demo-flow.', type: 'load', name: 'demo-flow', reason }]), cwd);
  const okLines = (def.renderResult as (r: ToolCallResult, o: object, t: object) => { render(w: number): string[] })(ok, {}, theme).render(100);
  assert.deepEqual(okLines, []);

  // Failure: renderResult shows the error
  const bad = await run(def, q([{ reasoning: 'Load nope.', type: 'load', name: 'nope', reason }]), cwd);
  const badRow = (def.renderResult as (r: ToolCallResult, o: object, t: object) => { render(w: number): string[] })(bad, {}, theme).render(100).join('\n');
  assert.match(badRow, /✗ skill/);
});

test('skill render rows: type:call success shows its concise outcome', async () => {
  const cwd = tmpWorkspace();
  const def = await makeTool();
  const theme = { fg: (_c: string, t: string) => t, bold: (t: string) => t };

  await withTempSkillsDir(async () => {
    const proposal = await run(def, q([{
      reasoning: 'Need a reusable deploy workflow.',
      type: 'call',
      skillType: 'deploy-render-flow',
      mode: 'auto',
      intent: 'deploy in stages and verify each stage before continuing',
    }]), cwd);
    const row = (def.renderResult as (r: ToolCallResult, o: object, t: object) => { render(w: number): string[] })(proposal, {}, theme).render(100).join('\n');
    assert.match(row, /✓ skill · \[PROPOSAL\]/);
  });
});

test('skill render rows: multi-query renders each operation and unlabeled reason', async () => {
  const def = await makeTool();
  const theme = { fg: (_c: string, t: string) => t, bold: (t: string) => t };
  const callLines = def.renderCall!(q([
    { reasoning: 'Load a.', type: 'load', name: 'a', reason: 'needs a' },
    { reasoning: 'Load b.', type: 'load', name: 'b', reason: 'needs b' },
  ]), theme).render(100);
  assert.equal(callLines.length, 5);
  assert.match(callLines[0]!, /2 queries.*sequential/);
  assert.match(callLines[1]!, /◆ skill · a/);
  assert.match(callLines[2]!, /needs a/);
  assert.match(callLines[3]!, /◆ skill · b/);
  assert.match(callLines[4]!, /needs b/);
  assert.doesNotMatch(callLines.join('\n'), /why:|reasoning:/);
});

test('skill render rows: single type:call shows callSkill action', async () => {
  const def = await makeTool();
  const theme = { fg: (_c: string, t: string) => t, bold: (t: string) => t };
  const callRow = def.renderCall!(q([{ reasoning: 'Manage deploy.', type: 'call', skillType: 'deploy-flow', mode: 'auto' }]), theme).render(100).join('\n');
  assert.match(callRow, /◆ skill · call:deploy-flow/);
});
