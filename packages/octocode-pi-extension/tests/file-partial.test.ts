import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { contextReads, seenFiles } from '../src/files/reads.js';
import { FileGuard } from '../src/files/guard.js';
import { registerFileTool } from '../src/files/tool.js';
import { projectedSession, toolCallMessage, toolResultMessage, type ToolArguments, type ToolResultMessage } from './doubles.js';
import { fakePermission, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

describe('file tool after a partial read', () => {
  it('refuses write and delete after a partial read but allows edit, and keeps the rule across a reload', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    const lines = Array.from({ length: 30 }, (_, index) => `line ${index + 1}`).join('\n');
    fs.writeFileSync(file, lines);
    const guard = new FileGuard();
    const { pi, tools, fire } = fakePi();
    registerFileTool(pi, guard, fakePermission());
    const tool = tools.get('file');
    const shown = 'line 11\nline 12\nline 13\nline 14\nline 15';
    await fire('tool_result', { toolName: 'read', input: { path: 'a.txt', offset: 11, limit: 5 }, content: [{ type: 'text', text: shown }], isError: false }, { cwd });
    const refusal = /you have seen lines 11–15 of its 30 lines[\s\S]*offset=1\)/;
    expect(guard.check(file, 'write', 'a.txt')).toMatch(refusal);
    // A delete needs only proof that the read was of the current file; an edit its exact match.
    expect(guard.check(file, 'delete', 'a.txt')).toBeUndefined();
    expect(guard.check(file, 'edit', 'a.txt')).toBeUndefined();
    const ctx = { cwd, hasUI: false };
    await expect(tool.execute('c', { queries: [{ reasoning: 'r', type: 'write', path: 'a.txt', content: 'x' }] }, undefined, undefined, ctx)).rejects.toThrow(refusal);
    expect(fs.readFileSync(file, 'utf8')).toBe(lines);

    // A reload rebuilds the guard from the messages the context shows: the partial flag comes back with them.
    const call = { id: 'a-1', message: toolCallMessage('c1', 'read', { path: 'a.txt', limit: 5 }) };
    const result = { id: 'r-1', message: toolResultMessage('c1', 'read', shown, { timestamp: Date.now() + 1000 }) };
    const reloaded = new FileGuard();
    reloaded.restore(contextReads(projectedSession(cwd, [call, result])));
    expect(reloaded.check(file, 'write', 'a.txt')).toMatch(/you have seen lines 1–5 of its 30 lines[\s\S]*offset=6\)/);
    expect(reloaded.check(file, 'delete', 'a.txt')).toBeUndefined();
    expect(reloaded.check(file, 'edit', 'a.txt')).toBeUndefined();

    // A whole read lifts it, and so does the tool's own write (recorded as a full read).
    await fire('tool_result', { toolName: 'read', input: { path: 'a.txt' }, content: [{ type: 'text', text: lines }], isError: false }, { cwd });
    expect(guard.check(file, 'write', 'a.txt')).toBeUndefined();
    await fire('tool_result', { toolName: 'read', input: { path: 'a.txt', offset: 2 }, content: [{ type: 'text', text: shown }], isError: false }, { cwd });
    await tool.execute('c', { queries: [{ reasoning: 'r', type: 'edit', path: 'a.txt', edits: [{ oldText: 'line 12', newText: 'LINE 12' }] }] }, undefined, undefined, ctx);
    expect(fs.readFileSync(file, 'utf8')).toContain('LINE 12');
    expect(guard.check(file, 'write', 'a.txt')).toBeUndefined();
  });

  it('counts a truncated read as partial, and a restored partial file keeps its flag', () => {
    const dir = tmp();
    const file = path.join(dir, 'a.txt');
    fs.writeFileSync(file, 'one');
    const text = [{ type: 'text', text: 'one' }];
    const read = (details?: unknown) => ({ role: 'toolResult', toolCallId: 'c', toolName: 'read', content: text, isError: false, timestamp: 100, ...(details ? { details } : {}) });
    const call = { id: 'a', message: { role: 'assistant', content: [{ type: 'toolCall', id: 'c', name: 'read', arguments: { path: file } }] } };
    expect(seenFiles([call, { id: 'r', message: read({ truncation: { truncated: true } }) }], dir).get(file)?.partial).toBe(true);
    expect(seenFiles([call, { id: 'r', message: read() }], dir).get(file)?.partial).toBeUndefined();
    // After a compaction the read-back message names the files it cut.
    const restored = { id: 'x', message: { role: 'custom', customType: 'octocode-restored-files', timestamp: 100, details: { files: [file], partial: [file] } } };
    expect(seenFiles([restored], dir).get(file)?.partial).toBe(true);
    const guard = new FileGuard();
    guard.restore([{ file, at: Date.now() + 1000, partial: true }]);
    expect(guard.check(file, 'write', 'a.txt')).toMatch(/you have seen only part of its 1 lines[\s\S]*offset=1\)/);
    expect(guard.check(file, 'delete', 'a.txt')).toBeUndefined();
    expect(guard.check(file, 'edit', 'a.txt')).toBeUndefined();
  });
});

const big = Array.from({ length: 3000 }, (_, index) => `line ${index + 1}`).join('\n') + '\n';
const text = (from: number, to: number) => big.split('\n').slice(from - 1, to).join('\n');
const setup = (content = big) => {
  const cwd = tmp();
  const file = path.join(cwd, 'big.txt');
  fs.writeFileSync(file, content);
  const guard = new FileGuard();
  const { pi, tools, fire } = fakePi();
  registerFileTool(pi, guard, fakePermission());
  const tool = tools.get('file');
  const read = (input: Record<string, unknown>, shown: string, extra: Record<string, unknown> = {}) =>
    fire('tool_result', { toolName: 'read', input: { path: 'big.txt', ...input }, content: [{ type: 'text', text: shown }], isError: false, ...extra }, { cwd });
  const run = (type: 'write' | 'delete', target = 'big.txt') =>
    tool.execute('c', { queries: [{ reasoning: 'r', type, path: target, ...(type === 'write' ? { content: 'new\n' } : {}) }] }, undefined, undefined, { cwd, hasUI: false });
  const edit = (edits: Array<{ oldText: string; newText: string }>) =>
    tool.execute('c', { queries: [{ reasoning: 'r', type: 'edit', path: 'big.txt', edits }] }, undefined, undefined, { cwd, hasUI: false });
  return { cwd, file, guard, read, run, edit, fire };
};
const cut = { truncation: { truncated: true, truncatedBy: 'lines', outputLines: 2000, totalLines: 3001 } };
const reloadCall = (id: string, name: string, args: ToolArguments) => ({ id: `a-${id}`, message: toolCallMessage(id, name, args) });
const reloadRead = (id: string, shown: string, at: number) => ({ id: `r-${id}`, message: toolResultMessage(id, 'read', shown, { timestamp: at }) });
const reloadResult = (id: string, shown: string, details?: ToolResultMessage['details']) => ({ id: `r-${id}`, message: toolResultMessage(id, 'read', shown, { timestamp: Date.now() + 1000, ...(details ? { details } : {}) }) });

describe('file tool after a partial read', () => {
  describe('a file Pi\'s read cannot show in one result', () => {
    it('allows a write once the ranges the agent read cover every line, and refuses it with the way forward before', async () => {
      const { file, read, run } = setup();
      await read({}, text(1, 2000), { details: cut });
      const refusal = await run('write').catch((error: Error) => error.message);
      expect(refusal).toMatch(/a write replaces the whole file, but you have seen lines 1–2000 of its 3000 lines\. Read the rest with `read` \(offset=2001\)/);
      // Never asks for what a big file cannot give.
      expect(refusal).not.toMatch(/whole file \(no offset|read the whole file/);
      expect(fs.readFileSync(file, 'utf8')).toBe(big);
      await read({ offset: 2001 }, text(2001, 3001));
      await run('write');
      expect(fs.readFileSync(file, 'utf8')).toBe('new\n');
    });

    it('adds up explicit ranges, whatever order they come in, and lets a whole read stand against a later range', async () => {
      const { file, guard, read, run } = setup();
      await read({ offset: 1501, limit: 1500 }, text(1501, 3000));
      await read({ offset: 1, limit: 1500 }, text(1, 1500));
      expect(guard.check(file, 'write', 'big.txt')).toBeUndefined();
      await read({}, big);
      await read({ offset: 5, limit: 3 }, text(5, 7));
      expect(guard.check(file, 'write', 'big.txt')).toBeUndefined();
      await run('write');
    });

    it('forgets ranges read before the file changed', async () => {
      const { file, guard, read } = setup();
      await read({ offset: 1, limit: 1500 }, text(1, 1500));
      fs.writeFileSync(file, `${big}more\n`);
      await read({ offset: 1501 }, text(1501, 2000), { details: { truncation: { truncated: true, outputLines: 500 } } });
      expect(guard.check(file, 'write', 'big.txt')).toMatch(/you have seen lines 1501–2000 of its 3001 lines[\s\S]*offset=1\)/);
    });

    it('allows a delete after any read that was not stale, a truncated one included', async () => {
      const { file, read, run } = setup();
      await read({}, text(1, 2000), { details: cut });
      await run('delete');
      expect(fs.existsSync(file)).toBe(false);
    });

    it('still refuses a delete after the file changed since the read', async () => {
      const { file, read, run } = setup();
      await read({}, text(1, 2000), { details: cut });
      fs.writeFileSync(file, 'changed\n');
      await expect(run('delete')).rejects.toThrow(/changed on disk since you last read it/);
      expect(fs.readFileSync(file, 'utf8')).toBe('changed\n');
    });

    it('counts an image read as seeing the whole file, for a delete and a write', async () => {
      const { cwd, file, fire, run } = setup('\u0089PNG bytes');
      const image = () => fire('tool_result', { toolName: 'read', input: { path: 'big.txt' }, content: [{ type: 'text', text: 'Read image file [image/png]' }, { type: 'image', data: 'eA==', mimeType: 'image/png' }], isError: false }, { cwd });
      await image();
      await run('write');
      expect(fs.readFileSync(file, 'utf8')).toBe('new\n');
      fs.writeFileSync(file, '\u0089PNG bytes');
      await image();
      await run('delete');
      expect(fs.existsSync(file)).toBe(false);
    });
  });
});

describe('file tool after a partial read', () => {
  describe('a file Pi\'s read cannot show in one result', () => {
    it('keeps the rule that a write or delete after an edit over outside changes needs a fresh read', async () => {
      const { file, guard, read, run } = setup('one\ntwo\n');
      await read({}, 'one\ntwo\n');
      fs.writeFileSync(file, 'one\ntwo\nthree\n');
      const fake = fakePi();
      registerFileTool(fake.pi, guard, fakePermission());
      await fake.tools.get('file').execute('c', { queries: [{ reasoning: 'r', type: 'edit', path: 'big.txt', edits: [{ oldText: 'two', newText: '2' }] }] }, undefined, undefined, { cwd: path.dirname(file), hasUI: false });
      await expect(run('write')).rejects.toThrow(/changed on disk since you last read it/);
      await expect(run('delete')).rejects.toThrow(/changed on disk since you last read it/);
      await read({}, fs.readFileSync(file, 'utf8'));
      await run('delete');
      expect(fs.existsSync(file)).toBe(false);
    });

    it('keeps a partial read partial through the tool\'s own edit: a later write over unseen lines is still refused', async () => {
      const { file, guard, read, run, edit } = setup();
      await read({ offset: 1, limit: 5 }, text(1, 5));
      // Same line count: the lines the agent saw are where it saw them.
      await edit([{ oldText: 'line 2\n', newText: 'LINE 2\n' }]);
      expect(guard.check(file, 'write', 'big.txt')).toMatch(/you have seen lines 1–5 of its 3000 lines[\s\S]*offset=6\)/);
      await expect(run('write')).rejects.toThrow(/offset=6\)/);
      // A different line count moved what lies below, so the ranges start over.
      await edit([{ oldText: 'LINE 2\n', newText: 'LINE 2\nextra\n' }]);
      expect(guard.check(file, 'write', 'big.txt')).toMatch(/you have seen only part of its 3001 lines[\s\S]*offset=1\)/);
      // An edit keeps the digest it wrote, so the next edit and a delete still pass; reading the rest lifts the write refusal.
      await edit([{ oldText: 'LINE 2\nextra\n', newText: 'LINE 2\n' }]);
      expect(guard.check(file, 'delete', 'big.txt')).toBeUndefined();
      await read({ offset: 1, limit: 1500 }, text(1, 1500));
      await read({ offset: 1501, limit: 1500 }, text(1501, 3000));
      expect(guard.check(file, 'write', 'big.txt')).toBeUndefined();
    });

    it('lets a write follow an edit of a whole read', async () => {
      const { file, guard, read, run, edit } = setup('one\ntwo\nthree\n');
      await read({}, 'one\ntwo\nthree\n');
      await edit([{ oldText: 'two', newText: '2\n2' }]);
      expect(guard.check(file, 'write', 'big.txt')).toBeUndefined();
      await run('write');
      expect(fs.readFileSync(file, 'utf8')).toBe('new\n');
    });

    it('counts the tool\'s own write as a whole view', async () => {
      const { file, guard, read, run } = setup();
      await read({ offset: 1, limit: 5 }, text(1, 5));
      expect(guard.check(file, 'write', 'big.txt')).toMatch(/offset=6\)/);
      await expect(run('write')).rejects.toThrow(/offset=6\)/);
      guard.restore([]);
      guard.record(file);
      await run('write');
      expect(guard.check(file, 'write', 'big.txt')).toBeUndefined();
    });

    it('does not let a range read lift the stale flag an edit over outside changes set', async () => {
      const { file, guard, read, run, edit } = setup('one\ntwo\nthree\nfour\nfive\nsix\n');
      await read({}, 'one\ntwo\nthree\nfour\nfive\nsix\n');
      fs.appendFileSync(file, 'outside\n');
      await edit([{ oldText: 'two', newText: '2' }]);
      expect(guard.check(file, 'write', 'big.txt')).toMatch(/changed on disk since you last read it/);
      await read({ offset: 1, limit: 3 }, 'one\n2\nthree');
      // Only what the new read showed counts: the outside line (and the rest) are unseen.
      expect(guard.check(file, 'write', 'big.txt')).toMatch(/you have seen lines 1–3 of its 7 lines[\s\S]*offset=4\)/);
      await expect(run('write')).rejects.toThrow(/offset=4\)/);
      await read({ offset: 4 }, 'four\nfive\nsix\noutside');
      await run('write');
      expect(fs.readFileSync(file, 'utf8')).toBe('new\n');
    });
  });
});

describe('file tool after a partial read', () => {
  describe('a file Pi\'s read cannot show in one result', () => {
    it('rebuilds the coverage after the tool\'s own change from the messages a reload shows', () => {
      const { cwd, file, guard } = setup();
      const changed = (id: string, outcome: Record<string, unknown>, at: number) => ({ id: `r-${id}`, message: toolResultMessage(id, 'file', 'ok', { timestamp: at, details: { outcomes: [{ ok: true, path: file, ...outcome }] } }) });
      const later = Date.now() + 1000;
      const reads = (messages: Parameters<typeof projectedSession>[1]) => contextReads(projectedSession(cwd, messages));
      const partial = [reloadCall('1', 'read', { path: file, limit: 5 }), reloadRead('1', text(1, 5), later)];
      const edit = (lineDelta: number) => [...partial, reloadCall('2', 'file', { queries: [] }), changed('2', { type: 'edit', lineDelta }, later + 10)];
      expect(reads(edit(0))).toEqual([{ file, at: later + 10, partial: true, lines: [[1, 5]] }]);
      expect(reads(edit(1))).toEqual([{ file, at: later + 10, partial: true }]);
      // An edit whose read is no longer in the context shows only its diff.
      expect(reads(edit(0).slice(2))).toEqual([{ file, at: later + 10, partial: true }]);
      const whole = [reloadCall('1', 'read', { path: file }), reloadRead('1', big, later), reloadCall('2', 'file', { queries: [] }), changed('2', { type: 'edit', lineDelta: 2 }, later + 10)];
      expect(reads(whole)).toEqual([{ file, at: later + 10 }]);
      expect(reads([...edit(0), reloadCall('3', 'file', { queries: [] }), changed('3', { type: 'write' }, later + 20)])).toEqual([{ file, at: later + 20 }]);
      guard.restore(reads(edit(0)));
      expect(guard.check(file, 'write', 'big.txt')).toMatch(/you have seen lines 1–5 of its 3000 lines[\s\S]*offset=6\)/);
      guard.restore(reads(whole));
      expect(guard.check(file, 'write', 'big.txt')).toBeUndefined();
    });

    it('rebuilds the covered lines from the messages a reload shows', () => {
      const { cwd, file, guard } = setup();
      const call = (id: string, args: ToolArguments) => reloadCall(id, 'read', { path: file, ...args });
      const messages = [call('1', {}), reloadResult('1', text(1, 2000), cut), call('2', { offset: 2001 }), reloadResult('2', text(2001, 3001))];
      const session = projectedSession(cwd, messages);
      expect(contextReads(session)).toEqual([{ file, at: expect.any(Number), partial: true, lines: [[1, Infinity]] }]);
      guard.restore(contextReads(session));
      expect(guard.check(file, 'write', 'big.txt')).toBeUndefined();
      guard.restore(contextReads(projectedSession(cwd, messages.slice(0, 2))));
      expect(guard.check(file, 'write', 'big.txt')).toMatch(/offset=2001/);
    });
  });
});
