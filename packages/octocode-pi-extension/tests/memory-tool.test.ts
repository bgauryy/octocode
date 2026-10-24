import fs from 'node:fs';
import path from 'node:path';
import type { AgentToolResult, ToolDefinition } from '@earendil-works/pi-coding-agent';
import type { TObject } from 'typebox';
import { describe, expect, it } from 'vitest';
import { MEMORY_HEADER } from '../src/memory/inject.js';
import { registerMemory } from '../src/memory/register.js';
import { MemoryStore, memoryLine } from '../src/memory/store.js';
import { Subcommands } from '../src/shared/commands.js';
import { textOf, toolContext } from './doubles.js';
import { fakePermission, fakePi, rendered, theme } from './fake-pi.js';
import { add, ctxFor, cwd, freshDb, useMemoryFixture } from './memory-fixture.js';

useMemoryFixture();

const setup = (isSubagent = false) => {
  const fake = fakePi();
  const commands = new Subcommands();
  registerMemory(fake.pi, { commands, isSubagent, permission: fakePermission() });
  return { ...fake, subcommands: commands, tool: fake.tools.get('memory') };
};
const run = (tool: ToolDefinition, params: Record<string, unknown>, ctx = ctxFor().ctx) => tool.execute('call', params, undefined, undefined, toolContext(ctx));
const text = (result: AgentToolResult<unknown>) => textOf(result.content[0]);
const untrusted = () => ctxFor({ trusted: false, confirms: [true] }).ctx;

describe('memory tool', () => {
  it('says when a search has more matches than limit, and does not when every match is shown', async () => {
    const { tool } = setup();
    for (const title of ['Alpha rule', 'Bravo habit', 'Charlie trick']) await run(tool, { op: 'set', title, body: 'gadget details' });
    const capped = text(await run(tool, { op: 'search', query: 'gadget', limit: 2 }));
    expect(capped.split('\n').filter((line) => line.startsWith('M'))).toHaveLength(2);
    expect(capped).toMatch(/\n… more memories match; raise limit/);
    const all = text(await run(tool, { op: 'search', query: 'gadget', limit: 3 }));
    expect(all.split('\n').filter((line) => line.startsWith('M'))).toHaveLength(3);
    expect(all).not.toContain('more memories match');
  });

  it('sets, searches, lists, gets and deletes', async () => {
    const { tool } = setup();
    expect(tool.executionMode).toBeUndefined();
    const saved = await run(tool, { op: 'set', title: 'Build with yarn', body: 'Run yarn build from the root', keywords: 'compile bundle', kind: 'procedure' });
    expect(text(saved)).toMatch(/^Saved M1 \(project, procedure\) Build with yarn/);
    expect(text(await run(tool, { op: 'search', query: 'compile' }))).toContain('M1 (project, procedure) Build with yarn');
    expect(text(await run(tool, { op: 'search', query: 'nothing here' }))).toBe('No memory matches.');
    expect(text(await run(tool, { op: 'get', id: 'M1' }))).toContain('keywords: compile bundle');
    expect(text(await run(tool, { op: 'list' }))).toContain('M1');
    expect(text(await run(tool, { op: 'set', id: 'M1', pinned: true }, ctxFor({ selects: ['Allow once'] }).ctx))).toMatch(/^Updated M1 \(project, procedure, pinned\)/);
    await run(tool, { op: 'set', title: 'Second memory', scope: 'global' }, ctxFor({ selects: ['Allow once'] }).ctx);
    expect(text(await run(tool, { op: 'list', limit: 1 }))).toContain('1 more; search, or raise limit');
    expect(text(await run(tool, { op: 'delete', id: 'm1' }, ctxFor({ selects: ['Allow once'] }).ctx))).toMatch(/^Deleted M1/);
    await expect(run(tool, { op: 'get', id: 'M1' })).rejects.toThrow(/No memory M1/);
    await expect(run(tool, { op: 'get', id: 'B1' })).rejects.toThrow(/not a memory id/);
    await expect(run(tool, { op: 'get' })).rejects.toThrow(/needs id/);
    await expect(run(tool, { op: 'search' })).rejects.toThrow(/needs query/);
    await expect(run(tool, { op: 'set', title: 'x', scope: 'all' })).rejects.toThrow(/project or global/);
    await expect(run(tool, { op: 'set', title: 'Key', body: `sk-${'a'.repeat(40)}` })).rejects.toThrow(/secret/);
    await expect(run(tool, { op: 'set', title: 'second MEMORY', scope: 'global' }, ctxFor({ selects: ['Allow once'] }).ctx)).rejects.toThrow(/Similar memory M2/);
  });

  it('saves a global or pinned memory at once in a trusted project, and never in an untrusted one', async () => {
    const { tool } = setup();
    // No dialog: a save the store would refuse (a secret, no title) fails the same way.
    const asked: string[] = [];
    const seen = ctxFor().ctx;
    Object.assign(seen.ui, { select: async (title: string) => (asked.push(title), 'Deny') });
    expect(text(await run(tool, { op: 'set', title: 'Shown', body: 'Line one\nLine two', pinned: true }, seen))).toMatch(/^Saved M1 \(project/);
    await expect(run(tool, { op: 'set', title: 'Key', body: `sk-${'a'.repeat(40)}`, pinned: true }, seen)).rejects.toThrow(/secret/);
    await expect(run(tool, { op: 'set', body: 'no title', pinned: true }, seen)).rejects.toThrow(/needs a title/);
    expect(asked).toEqual([]);
    expect(text(await run(tool, { op: 'set', title: 'Global one', scope: 'global' }, ctxFor({ hasUI: false }).ctx))).toMatch(/^Saved M2 \(global/);
    expect(text(await run(tool, { op: 'set', title: 'Project one' }))).toMatch(/^Saved M3 \(project/);
    fs.mkdirSync(path.join(cwd, '.claude'), { recursive: true });
    fs.writeFileSync(path.join(cwd, '.claude', 'settings.json'), '{"hooks":{}}');
    await expect(run(tool, { op: 'set', title: 'Sneaky', scope: 'global' }, untrusted())).rejects.toThrow(/untrusted project/);
    await expect(run(tool, { op: 'set', title: 'Sneaky', pinned: true }, untrusted())).rejects.toThrow(/untrusted project/);
    await expect(run(tool, { op: 'set', id: 'M2', body: 'changed' }, untrusted())).rejects.toThrow(/untrusted project/);
    await expect(run(tool, { op: 'delete', id: 'M2' }, untrusted())).rejects.toThrow(/untrusted project/);
    expect(text(await run(tool, { op: 'set', title: 'Plain project note' }, untrusted()))).toMatch(/^Saved M4 \(project[\s\S]*hides project memories from reads until the user \/trusts it/);
    // Hidden project memories: changing them by id, or colliding with one, reveals nothing about them.
    for (const params of [{ op: 'set', id: 'M3', body: 'x' }, { op: 'delete', id: 'M3' }]) await expect(run(tool, params, untrusted())).rejects.toThrow(/^No memory M3 here/);
    await expect(run(tool, { op: 'set', title: 'Project one' }, untrusted())).rejects.toThrow(/^A similar project memory \(M3\) exists, hidden[^"]*$/);
    // Reads match auto-injection: only global memories, with a note why.
    const listed = text(await run(tool, { op: 'list' }, untrusted()));
    expect(listed).toContain('M2');
    expect(listed).not.toContain('M3');
    expect(listed).toMatch(/untrusted project: project memories are hidden/);
    expect(text(await run(tool, { op: 'search', query: 'project one' }, untrusted()))).not.toContain('M3');
    await expect(run(tool, { op: 'get', id: 'M3' }, untrusted())).rejects.toThrow(/No memory M3/);
  });
});

describe('memory tool in a subagent and in the terminal', () => {
  it('is read-only in a subagent', async () => {
    const { tool, subcommands } = setup(true);
    expect(tool.parameters.properties.op.enum).toEqual(['search', 'get', 'list']);
    // Every text the model reads says read-only and none offers a write.
    expect(tool.description).toMatch(/read-only/i);
    expect(tool.promptGuidelines.join(' ')).toMatch(/read-only/i);
    for (const line of [tool.promptSnippet, ...tool.promptGuidelines]) expect(line).not.toMatch(/\b(save|set|updat\w*|delete)\b/i);
    await expect(run(tool, { op: 'set', title: 'x' })).rejects.toThrow(/read-only in a subagent/);
    await expect(run(tool, { op: 'delete', id: 'M1' })).rejects.toThrow(/read-only in a subagent/);
    expect(subcommands.get('memory')).toBeUndefined();
  });

  it('describes every field and warns that a memory may be stale', () => {
    for (const subagent of [false, true]) {
      const { tool } = setup(subagent);
      // A memory read by search (not only the injected header) may be stale: the guideline says to verify it.
      expect(tool.promptGuidelines.join(' ')).toMatch(/may be stale; verify it against the code/);
      expect(MEMORY_HEADER).toMatch(/may be stale/);
      const parameters: TObject = tool.parameters;
      for (const [name, schema] of Object.entries(parameters.properties)) expect('description' in schema && schema.description, name).toBeTruthy();
    }
  });

  it('renders the call and a collapsed result', () => {
    const { tool } = setup();
    expect(rendered(tool.renderCall({ op: 'search', query: 'rewind flow' }, theme, {}))).toBe('○ Memory(search "rewind flow")');
    expect(rendered(tool.renderCall({ op: 'set', title: 'red \u001b[31mtext\u001b[0m\nnext' }, theme, {}))).toBe('○ Memory(set "red text")');
    expect(rendered(tool.renderCall({ op: 'get', id: 'M7' }, theme, {}))).toBe('○ Memory(get M7)');
    expect(setup().tool.promptGuidelines.join(' ')).toMatch(/scope global or pinned only when the user asks/);
    const hits = { content: [{ type: 'text', text: 'M1 a\nM2 b' }], details: { ids: [1, 2] } };
    expect(rendered(tool.renderResult(hits, { expanded: false }, theme, { args: { op: 'search' }, expanded: false }))).toBe('  ⎿  2 hits\n     M1 a\n     M2 b');
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'No memory matches.' }], details: { ids: [] } }, { expanded: false }, theme, { args: { op: 'search' } }))).toBe('  ⎿  No matches');
    const listed = { content: [{ type: 'text', text: 'M1 a' }], details: { ids: [1], total: 4 } };
    expect(rendered(tool.renderResult(listed, { expanded: false }, theme, { args: { op: 'list' } }))).toBe('  ⎿  1 of 4 memories\n     M1 a');
    const saved = { content: [{ type: 'text', text: 'Saved M7 [project fact] title' }], details: { ids: [7] } };
    expect(rendered(tool.renderResult(saved, { expanded: false }, theme, { args: { op: 'set' } }))).toBe('  ⎿  Saved M7 [project fact] title');
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'set needs a title' }] }, { expanded: false }, theme, { args: { op: 'set' }, isError: true }))).toBe('  ⎿  Error: set needs a title');
  });
});

describe('memory lines the model reads', () => {
  const long = `${'sentence about gadgets. '.repeat(30)}the end`;
  const pointer = 'memory get M1';

  it('writes the whole body unless a caller asks for a clip, which the terminal views do', () => {
    const memory = add(new MemoryStore(freshDb(), 'repo-a'), { title: 'Gadgets', body: long });
    expect(memoryLine(memory)).toBe(`M${memory.id} (project, fact) Gadgets — ${long}`);
    expect(memoryLine(memory, 20)).toBe(`M${memory.id} (project, fact) Gadgets — ${long.slice(0, 19)}…`);
  });

  it('ends a search or list line whose body is cut with the call that reads it whole, and leaves a short body bare', async () => {
    const tool = setup().tool;
    await run(tool, { op: 'set', title: 'Gadgets', body: long });
    await run(tool, { op: 'set', title: 'Widgets', body: 'short gadget note' });
    for (const params of [{ op: 'search', query: 'gadgets' }, { op: 'list' }]) {
      const lines = text(await run(tool, params)).split('\n');
      const gadgets = lines.find((line) => line.includes('Gadgets'))!;
      expect(gadgets).toMatch(/… \(memory get M1\)$/);
      expect(lines.find((line) => line.includes('Widgets'))).toMatch(/short gadget note$/);
    }
    // The call the line names returns the body exactly.
    expect(text(await run(tool, { op: 'get', id: 'M1' })).endsWith(`\n\n${long}`)).toBe(true);
  });

  it('ends a saved line whose body is cut with the same call, and shows a deleted memory whole: it can no longer be read', async () => {
    const tool = setup().tool;
    expect(text(await run(tool, { op: 'set', title: 'Gadgets', body: long }))).toMatch(new RegExp(`… \\(${pointer}\\)$`));
    expect(text(await run(tool, { op: 'set', id: 'M1', title: 'Gadgets', body: 'now short' }))).toMatch(/^Updated M1 .* — now short$/);
    await run(tool, { op: 'set', id: 'M1', title: 'Gadgets', body: long });
    expect(text(await run(tool, { op: 'delete', id: 'M1' }, ctxFor({ selects: ['Allow once'] }).ctx))).toBe(`Deleted M1 (project, fact) Gadgets — ${long}`);
  });
});

