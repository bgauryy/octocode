import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { closeSharedAgentDb } from '../src/agentdb/db.js';
import { memoryTemplate, parseTemplate } from '../src/memory/command.js';
import { MEMORY_HEADER, MEMORY_MESSAGE_TYPE } from '../src/memory/inject.js';
import { memoryStore, registerMemory } from '../src/memory/register.js';
import { Subcommands } from '../src/shared/commands.js';
import { fakePermission, fakePi } from './fake-pi.js';
import { add, ctxFor, cwd, root, useMemoryFixture, WORDS } from './memory-fixture.js';

useMemoryFixture();

const setupInjection = (isSubagent = false) => {
  const fake = fakePi();
  registerMemory(fake.pi, { commands: new Subcommands(), isSubagent, permission: fakePermission() });
  return fake;
};
const seed = () => {
  const store = memoryStore(cwd);
  const project = add(store, { title: 'Rewind uses checkpoints', body: 'checkpoints live per session' });
  const global = add(store, { title: 'Checkpoints are cheap', scope: 'global' });
  return { store, project, global };
};
const setupCommand = () => {
  const fake = fakePi();
  const commands = new Subcommands();
  registerMemory(fake.pi, { commands, isSubagent: false, permission: fakePermission() });
  const command = commands.get('memory')!;
  return { command, commands };
};
const option = (title: string) => `Project · M1 (project, decision) ${title} — `;
/** The hidden memory message a before_agent_start result injects, with the ids of the memories it carries. */
function injected(result: unknown) {
  const message = typeof result === 'object' && result !== null && 'message' in result ? result.message : undefined;
  if (typeof message !== 'object' || message === null || !('content' in message) || typeof message.content !== 'string') throw new Error('expected an injected text message');
  const details = 'details' in message ? message.details : undefined;
  const ids: unknown = typeof details === 'object' && details !== null && 'ids' in details ? details.ids : undefined;
  if (!Array.isArray(ids)) throw new Error('expected the injected memory ids');
  return { message, content: message.content, ids };
}

describe('memory auto-injection', () => {
  it('injects a hidden octocode-memory message once per branch and bumps usage', async () => {
    const fake = setupInjection();
    const { store, project, global } = seed();
    const { ctx, branch } = ctxFor();
    const result = injected(await fake.fire('before_agent_start', { prompt: 'fix the rewind checkpoints bug' }, ctx));
    expect(result.message).toMatchObject({ customType: MEMORY_MESSAGE_TYPE, display: false });
    expect(result.ids).toHaveLength(2);
    expect(result.ids).toEqual(expect.arrayContaining([project.id, global.id]));
    expect(result.content.startsWith(MEMORY_HEADER)).toBe(true);
    expect(store.get(project.id)?.useCount).toBe(1);
    branch.push({ type: 'custom_message', ...result.message });
    expect(await fake.fire('before_agent_start', { prompt: 'checkpoints again' }, ctx)).toBeUndefined();
  });

  it('injects only global memories in an untrusted project', async () => {
    const fake = setupInjection();
    const { global } = seed();
    fs.mkdirSync(path.join(cwd, '.claude'), { recursive: true });
    fs.writeFileSync(path.join(cwd, '.claude', 'settings.json'), '{"hooks":{}}');
    const { ctx } = ctxFor({ trusted: false });
    expect(injected(await fake.fire('before_agent_start', { prompt: 'rewind checkpoints' }, ctx)).ids).toEqual([global.id]);
  });

  it('is off with OCTOCODE_MEMORY_AUTO=0 or the stored setting, and survives a broken DB', async () => {
    const fake = setupInjection();
    const { store } = seed();
    vi.stubEnv('OCTOCODE_MEMORY_AUTO', '0');
    expect(await fake.fire('before_agent_start', { prompt: 'rewind' }, ctxFor().ctx)).toBeUndefined();
    vi.stubEnv('OCTOCODE_MEMORY_AUTO', '');
    store.setAutoSetting(false);
    expect(await fake.fire('before_agent_start', { prompt: 'rewind' }, ctxFor().ctx)).toBeUndefined();
    closeSharedAgentDb();
    const foreign = path.join(root, 'foreign.db');
    fs.writeFileSync(foreign, 'not a database');
    vi.stubEnv('OCTOCODE_AGENT_DB', foreign);
    expect(await fake.fire('before_agent_start', { prompt: 'rewind' }, ctxFor().ctx)).toBeUndefined();
  });

  it('gives a subagent at most 3 hits and no pinned memories (its parent holds those)', async () => {
    const fake = setupInjection(true);
    const store = memoryStore(cwd);
    const pinned = add(store, { title: 'Always on', body: 'unrelated', pinned: true });
    for (let i = 0; i < 6; i++) add(store, { title: `Parser ${WORDS[i]}`, body: `parser ${WORDS[i + 10]} ${WORDS[i + 20]}` });
    const { ids } = injected(await fake.fire('before_agent_start', { prompt: 'parser' }, ctxFor().ctx));
    expect(ids).toHaveLength(3);
    expect(ids).not.toContain(pinned.id);
  });

  it('injects no renderer: the message is hidden', () => {
    expect(setupInjection().renderers.has(MEMORY_MESSAGE_TYPE)).toBe(false);
  });
});

describe('/octocode memory', () => {
  it('round-trips the editor template', () => {
    const input = parseTemplate(memoryTemplate({ title: 'T # not a comment', kind: 'gotcha', keywords: 'a b', scope: 'global', body: 'line1\nline2', pinned: true }));
    expect(input).toEqual({ title: 'T # not a comment', kind: 'gotcha', keywords: 'a b', scope: 'global', body: 'line1\nline2', pinned: true });
    expect(() => parseTemplate('kind: nope\n---\n')).toThrow(/kind must be/);
    expect(() => parseTemplate('scope: all\n---\n')).toThrow(/scope must be/);
    expect(parseTemplate('title: only')).toEqual({ title: 'only', body: '' });
  });

  it('prints grouped text headless, adds, searches, and toggles auto', async () => {
    const { command, commands } = setupCommand();
    const { ctx } = ctxFor({ hasUI: false });
    await command.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toBe('Memories (auto on): none.');
    await command.handler('add Deploy with care — run the smoke tests first', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/^Saved M1 \(project, fact\) Deploy with care — run the smoke/);
    memoryStore(cwd).set({ title: 'Global pin', scope: 'global', pinned: true, author: 'user' });
    await command.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toBe(['Memories (auto on):', 'Pinned:', '  M2 (global, fact, pinned) Global pin', 'Project:', '  M1 (project, fact) Deploy with care — run the smoke tests first'].join('\n'));
    await command.handler('search smoke', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toContain('M1');
    await command.handler('search', ctx);
    expect(ctx.ui.notes.at(-1)!.type).toBe('warning');
    await command.handler('add', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/^Usage/);
    await command.handler('add deploy with CARE', ctx);
    expect(ctx.ui.notes.at(-1)).toMatchObject({ type: 'warning', message: expect.stringContaining('Similar memory M1') });
    await command.handler('auto off', ctx);
    expect(memoryStore(cwd).autoSetting()).toBe(false);
    await command.handler('auto', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toContain('is off');
    vi.stubEnv('OCTOCODE_MEMORY_AUTO', 'off');
    await command.handler('auto on', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toContain('Still off here');
    await command.handler('bogus', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/^Usage/);
    expect(commands.complete('memory au')?.map((item) => item.value)).toEqual(['memory auto on', 'memory auto off']);
  });

  it('shows the last injection on this branch', async () => {
    const { command } = setupCommand();
    const { ctx, branch } = ctxFor();
    await command.handler('last', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/No memories were injected/);
    branch.push({ type: 'custom_message', customType: MEMORY_MESSAGE_TYPE, content: `${MEMORY_HEADER}\nM4 x`, details: { ids: [4] } });
    await command.handler('last', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toBe(`${MEMORY_HEADER}\nM4 x`);
  });
});

describe('/octocode memory picker', () => {
  it('adds through the editor and edits, pins, moves and deletes from the picker', async () => {
    const { command } = setupCommand();
    const store = memoryStore(cwd);
    const created = parseTemplate('title: Editor made\nkind: decision\n---\nbody text');
    const { ctx, editorCalls } = ctxFor({
      editor: [memoryTemplate({ ...created, title: 'Editor made', kind: 'decision', keywords: '', scope: 'project', body: 'body text', pinned: false }), 'title: Edited title\nkind: decision\n---\nnew body'],
      selects: ['+ Add a memory'],
    });
    await command.handler('', ctx);
    expect(store.get(1)).toMatchObject({ title: 'Editor made', kind: 'decision', author: 'user' });
    expect(editorCalls[0].prefill).toContain('title: ');

    ctx.ui.selects.push(option('Editor made') + 'body text', 'Edit');
    ctx.ui.selects.push(undefined);
    await command.handler('', ctx);
    expect(store.get(1)).toMatchObject({ title: 'Edited title', body: 'new body' });

    ctx.ui.selects.push(option('Edited title') + 'new body', 'Pin', 'Pinned · M1 (project, decision, pinned) Edited title — new body', 'Move to global', undefined);
    await command.handler('', ctx);
    expect(store.get(1)).toMatchObject({ pinned: true, scope: 'global' });

    ctx.ui.selects.push('Pinned · M1 (global, decision, pinned) Edited title — new body', 'Show', undefined);
    await command.handler('search edited', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toContain('author: user');

    ctx.ui.selects.push('Pinned · M1 (global, decision, pinned) Edited title — new body', 'Delete');
    ctx.ui.confirms.push(false);
    ctx.ui.selects.push('Pinned · M1 (global, decision, pinned) Edited title — new body', 'Delete', undefined);
    ctx.ui.confirms.push(true);
    await command.handler('', ctx);
    expect(store.get(1)).toBeUndefined();
    expect(ctx.ui.notes.at(-1)!.message).toBe('Deleted M1.');
  });
});
