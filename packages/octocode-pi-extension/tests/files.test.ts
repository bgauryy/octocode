import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { initTheme } from '@earendil-works/pi-coding-agent';
import { diffStats, querySize } from '../src/files/render.js';
import { FileGuard } from '../src/files/guard.js';
import { formatOutcomes, registerFileTool } from '../src/files/tool.js';
import { Checkpoints } from '../src/files/checkpoint.js';
import { sha256 } from '../src/shared/atomic.js';
import { capChars } from '../src/shared/util.js';
import { fakeCtx, fakePermission } from './fake-pi.js';
import { tmp } from './helpers.js';
import { theme } from './fake-pi.js';

describe('file tool', () => {
  it('stops applying a batch once it is aborted: a later delete does not run', async () => {
    const cwd = tmp();
    fs.writeFileSync(path.join(cwd, 'b.txt'), 'keep');
    let tool: { execute: (...args: unknown[]) => Promise<unknown> } | undefined;
    const guard = new FileGuard();
    registerFileTool({ registerTool: (t: typeof tool) => (tool = t), on: () => undefined } as never, guard, fakePermission());
    const controller = new AbortController();
    const record = guard.record.bind(guard);
    // Cancel after the first write finishes, before the next query. Exercise a real signal and an observable boundary.
    vi.spyOn(guard, 'record').mockImplementation((file, known) => {
      record(file, known);
      if (file === path.join(cwd, 'a.txt')) controller.abort();
    });
    const batch = { queries: [{ reasoning: 'r', type: 'write', path: 'a.txt', content: 'x' }, { reasoning: 'r', type: 'delete', path: 'b.txt' }] };
    const result = await tool!.execute('c', batch, controller.signal, undefined, fakeCtx({ cwd, hasUI: false }));
    expect(result).toMatchObject({ details: { outcomes: [{ ok: true }, { ok: false, message: 'Not applied: the batch was aborted.' }] } });
    expect(fs.readFileSync(path.join(cwd, 'a.txt'), 'utf8')).toBe('x');
    expect(fs.readFileSync(path.join(cwd, 'b.txt'), 'utf8')).toBe('keep');
  });

  it('takes any number of queries in a batch and refuses an empty one with a clear message', async () => {
    const cwd = tmp();
    let tool: { parameters: { properties: { queries: Record<string, unknown> } }; execute: (...args: unknown[]) => Promise<unknown> } | undefined;
    registerFileTool({ registerTool: (t: typeof tool) => (tool = t), on: () => undefined } as never, new FileGuard(), fakePermission());
    // The schema does not cap or floor the list (a validation refusal echoes every argument): the tool itself decides.
    expect(tool!.parameters.properties.queries).not.toHaveProperty('maxItems');
    expect(tool!.parameters.properties.queries).not.toHaveProperty('minItems');
    const queries = Array.from({ length: 12 }, (_, index) => ({ reasoning: 'r', type: 'write', path: `f${index}.txt`, content: String(index) }));
    const result = await tool!.execute('c', { queries }, undefined, undefined, fakeCtx({ cwd, hasUI: false }));
    expect((result as { details: { outcomes: Array<{ ok: boolean }> } }).details.outcomes).toHaveLength(12);
    expect(fs.readFileSync(path.join(cwd, 'f11.txt'), 'utf8')).toBe('11');
    await expect(tool!.execute('c', { queries: [] }, undefined, undefined, fakeCtx({ cwd, hasUI: false }))).rejects.toThrow('No queries');
  });

  it('records the digest of what the file tool wrote, so the next change needs no re-read but a later rewrite is still caught', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const guard = new FileGuard();
    guard.record(file);
    let tool: { execute: (...args: unknown[]) => Promise<unknown> } | undefined;
    registerFileTool({ registerTool: (t: typeof tool) => (tool = t), on: () => undefined } as never, guard, fakePermission());
    const ctx = { cwd, hasUI: false };
    await tool!.execute('c', { queries: [{ reasoning: 'r', type: 'write', path: 'b.txt', content: 'new' }] }, undefined, undefined, ctx);
    await tool!.execute('c', { queries: [{ reasoning: 'r', type: 'edit', path: 'a.txt', edits: [{ oldText: 'one', newText: 'two' }] }] }, undefined, undefined, ctx);
    expect(guard.check(file, 'write', 'a.txt')).toBeUndefined();
    expect(guard.check(path.join(cwd, 'b.txt'), 'write', 'b.txt')).toBeUndefined();
    const { atime, mtime } = fs.statSync(file);
    fs.writeFileSync(file, 'TWO');
    fs.utimesSync(file, atime, mtime);
    await expect(tool!.execute('c', { queries: [{ reasoning: 'r', type: 'write', path: 'a.txt', content: 'x' }] }, undefined, undefined, ctx)).rejects.toThrow(/changed on disk/);
    expect(fs.readFileSync(file, 'utf8')).toBe('TWO');
  });

  it('asks for a change in the home folder when the session started there: home is no workspace', async () => {
    const home = tmp();
    vi.stubEnv('HOME', home);
    const file = path.join(home, 'notes.txt');
    fs.writeFileSync(file, 'one');
    const guard = new FileGuard();
    guard.record(file);
    let tool: { execute: (...args: unknown[]) => Promise<unknown> } | undefined;
    const permission = fakePermission();
    const check = vi.spyOn(permission, 'check').mockImplementation((async (_ctx: unknown, request: { action: string }) => (request.action === 'file.outside' ? { allowed: false, message: 'denied here', via: 'mode' } : { allowed: true, via: 'mode' })) as never);
    registerFileTool({ registerTool: (t: typeof tool) => (tool = t), on: () => undefined } as never, guard, permission);
    await expect(tool!.execute('c', { queries: [{ reasoning: 'r', type: 'edit', path: 'notes.txt', edits: [{ oldText: 'one', newText: 'two' }] }] }, undefined, undefined, { cwd: home, hasUI: false })).rejects.toThrow(/outside any workspace \(the home folder is not one\)\. denied here/);
    expect(check).toHaveBeenCalledWith(expect.anything(), expect.objectContaining({ action: 'file.outside', what: [`edit ${fs.realpathSync(file)}`, 'workspace: none (the home folder is not a workspace)'] }));
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
  });

  describe('an edit of a file changed on disk since the read', () => {
    function setup(content: string) {
      const cwd = tmp();
      const file = path.join(cwd, 'a.ts');
      fs.writeFileSync(file, content);
      const guard = new FileGuard();
      guard.record(file, { sha256: sha256(content), size: Buffer.byteLength(content) });
      let tool: { execute: (...args: unknown[]) => Promise<unknown> } | undefined;
      registerFileTool({ registerTool: (t: typeof tool) => (tool = t), on: () => undefined } as never, guard, fakePermission());
      const edit = (type: 'edit' | 'write' | 'delete', edits: Array<{ oldText: string; newText: string }>) =>
        tool!.execute('c', { queries: [{ reasoning: 'r', type, path: 'a.ts', edits, ...(type === 'write' ? { content: 'x' } : {}) }] }, undefined, undefined, { cwd, hasUI: false });
      return { file, guard, edit };
    }

    it('applies when a formatter ran in between and every oldText still matches once, then tracks the new content', async () => {
      const { file, guard, edit } = setup('const a=1;\nconst b=2;\n');
      fs.writeFileSync(file, 'const a = 1;\nconst b=2;\n// formatted\n');
      await edit('edit', [{ oldText: 'const b=2;', newText: 'const b = 3;' }]);
      expect(fs.readFileSync(file, 'utf8')).toBe('const a = 1;\nconst b = 3;\n// formatted\n');
      // The formatter's changes were never read: a whole-file write would overwrite them unseen.
      expect(guard.check(file, 'write', 'a.ts')).toMatch(/read/);
      await edit('edit', [{ oldText: 'formatted', newText: 'done' }]);
      expect(fs.readFileSync(file, 'utf8')).toContain('// done');
    });

    it('refuses and names an anchor another writer removed, leaving the file alone', async () => {
      const { file, edit } = setup('const a=1;\nconst b=2;\n');
      fs.writeFileSync(file, 'const a=1;\nconst b = 2;\n');
      await expect(edit('edit', [{ oldText: 'const a=1;', newText: 'x' }, { oldText: 'const b=2;', newText: 'y' }])).rejects.toThrow(/edits\[1\] \(oldText "const b=2;"\) is no longer in it[\s\S]*Read it again with `read`/);
      expect(fs.readFileSync(file, 'utf8')).toBe('const a=1;\nconst b = 2;\n');
    });

    it('refuses an anchor that now matches more than once', async () => {
      const { file, edit } = setup('let x = 1;\nlet y = 2;\n');
      fs.writeFileSync(file, 'let x = 1;\nlet y = 2;\nlet x = 1;\n');
      await expect(edit('edit', [{ oldText: 'let x = 1;', newText: 'z' }])).rejects.toThrow(/edits\[0\] \(oldText "let x = 1;"\) now matches 2 times/);
      expect(fs.readFileSync(file, 'utf8')).toBe('let x = 1;\nlet y = 2;\nlet x = 1;\n');
    });

    it('refuses a write and a delete', async () => {
      const { file, edit } = setup('one\n');
      fs.writeFileSync(file, 'formatted one\n');
      await expect(edit('write', [])).rejects.toThrow(/changed on disk since you last read it/);
      await expect(edit('delete', [])).rejects.toThrow(/changed on disk since you last read it/);
      expect(fs.readFileSync(file, 'utf8')).toBe('formatted one\n');
    });

    it('still refuses placeholder text copied into the new text', async () => {
      const { file, edit } = setup('one\n');
      fs.writeFileSync(file, 'one\ntwo\n');
      await expect(edit('edit', [{ oldText: 'two', newText: capChars('z'.repeat(100), 10) }])).rejects.toThrow(/placeholder Octocode puts/);
      expect(fs.readFileSync(file, 'utf8')).toBe('one\ntwo\n');
    });
  });

  it('writes, re-reads and edits a ~ path outside the workspace at the file Pi touches once the user allows it, checkpointing it', async () => {
    const home = tmp();
    const cwd = tmp();
    vi.stubEnv('HOME', home);
    try {
      const file = path.join(home, '.bashrc');
      fs.writeFileSync(file, 'one');
      const guard = new FileGuard();
      const checkpoints = new Checkpoints();
      const store = tmp();
      checkpoints.open(store);
      const handlers = new Map<string, (event: unknown, ctx: unknown) => Promise<void>>();
      let tool: { execute: (...args: unknown[]) => Promise<unknown> } | undefined;
      registerFileTool({ registerTool: (t: typeof tool) => (tool = t), on: (name: string, handler: never) => handlers.set(name, handler) } as never, guard, fakePermission(), checkpoints);
      // ~ is outside the workspace (cwd): each change asks the user.
      const ctx = fakeCtx({ cwd, ui: { selects: ['Allow once', 'Allow once', 'Allow once'] } });
      await handlers.get('tool_result')!({ toolName: 'read', input: { path: '~/.bashrc' }, content: [{ type: 'text', text: 'one' }], isError: false }, ctx);
      await tool!.execute('c', { queries: [{ reasoning: 'r', type: 'edit', path: '~/.bashrc', edits: [{ oldText: 'one', newText: 'two' }] }] }, undefined, undefined, ctx);
      await tool!.execute('c', { queries: [{ reasoning: 'r', type: 'write', path: '~/.bashrc', content: 'three' }] }, undefined, undefined, ctx);
      await tool!.execute('c', { queries: [{ reasoning: 'r', type: 'write', path: '~/new.txt', content: 'n' }] }, undefined, undefined, ctx);
      expect(fs.readFileSync(file, 'utf8')).toBe('three');
      expect(fs.readFileSync(path.join(home, 'new.txt'), 'utf8')).toBe('n');
      expect(fs.existsSync(path.join(cwd, '~'))).toBe(false);
      const journaled = fs.readFileSync(path.join(store, 'journal.jsonl'), 'utf8').split('\n').filter(Boolean).map((line) => (JSON.parse(line) as { path: string }).path);
      expect(new Set(journaled)).toEqual(new Set([file, path.join(home, 'new.txt')]));
      expect(ctx.ui.selects).toEqual([]);
    } finally {
      vi.unstubAllEnvs();
    }
  });

  it('refuses paths outside the workspace without a confirming user, and Octocode\'s state folder always', async () => {
    const cwd = tmp();
    const outside = tmp();
    let tool: { execute: (...args: unknown[]) => Promise<{ content: Array<{ text: string }> }> } | undefined;
    registerFileTool({ registerTool: (t: typeof tool) => (tool = t), on: () => undefined } as never, new FileGuard(), fakePermission());
    const write = (target: string, ctx: unknown) => tool!.execute('c', { queries: [{ reasoning: 'r', type: 'write', path: target, content: 'x' }] }, undefined, undefined, ctx);
    const target = path.join(outside, 'x.txt');
    await expect(write(target, { cwd, hasUI: false })).rejects.toThrow(/outside the workspace[\s\S]*needs the user's approval \(workspace mode\) and no one can be asked here/);
    const denied = fakeCtx({ cwd, ui: { selects: ['Deny'] } });
    await expect(write(target, denied)).rejects.toThrow(/The user denied changing a file outside the workspace.*Do not retry it/);
    expect(fs.existsSync(target)).toBe(false);
    // Octocode's home (trust decisions, memory, sessions) is refused even when the user would allow it, in full too.
    const state = path.join(process.env['OCTOCODE_HOME']!, 'pi-trust.json');
    const allowing = fakeCtx({ cwd, ui: { selects: ['Allow once'] } });
    await expect(write(state, allowing)).rejects.toThrow(/Octocode's state folder/);
    expect(allowing.ui.selects).toEqual(['Allow once']);
    let full: typeof tool;
    registerFileTool({ registerTool: (t: typeof tool) => (full = t), on: () => undefined } as never, new FileGuard(), fakePermission('full'));
    await expect(full!.execute('c', { queries: [{ reasoning: 'r', type: 'write', path: state, content: 'x' }] }, undefined, undefined, { cwd, hasUI: false })).rejects.toThrow(/Octocode's state folder/);
    expect(fs.existsSync(state)).toBe(false);
    expect((await write('inside.txt', { cwd, hasUI: false })).content[0]!.text).toMatch(/OK write inside\.txt/);
  });

  it('keeps a change that succeeded when its checkpoint cannot be saved, and says so once', async () => {
    const cwd = tmp();
    const checkpoints = new Checkpoints();
    checkpoints.open(tmp());
    vi.spyOn(checkpoints, 'settle').mockRejectedValue(new Error('disk full'));
    let tool: { execute: (...args: unknown[]) => Promise<{ content: Array<{ text: string }> }> } | undefined;
    registerFileTool({ registerTool: (t: typeof tool) => (tool = t), on: () => undefined } as never, new FileGuard(), fakePermission(), checkpoints);
    const ctx = fakeCtx({ cwd });
    const first = await tool!.execute('c', { queries: [{ reasoning: 'r', type: 'write', path: 'a.txt', content: 'a' }, { reasoning: 'r', type: 'write', path: 'b.txt', content: 'b' }] }, undefined, undefined, ctx);
    expect(first.content[0]!.text).toMatch(/1\. OK write a\.txt.*checkpoint not saved[\s\S]*2\. OK write b\.txt.*checkpoint not saved/);
    expect(fs.readFileSync(path.join(cwd, 'a.txt'), 'utf8')).toBe('a');
    expect(ctx.ui.notes.filter((note) => /could not save a checkpoint \(disk full\)/.test(note.message))).toHaveLength(1);
  });

  it('reports each query independently', () => {
    expect(
      formatOutcomes([
        { type: 'write', path: 'a', reasoning: 'r', ok: true, message: 'Wrote a' },
        { type: 'edit', path: 'b', reasoning: 'r', ok: false, message: 'no match' },
      ]),
    ).toBe('1. OK write a: Wrote a\n2. FAILED edit b: no match');
    expect(
      formatOutcomes([
        { type: 'edit', path: 'src/a.ts', reasoning: 'r', ok: true, message: 'Successfully replaced 2 block(s) in src/a.ts.' },
        { type: 'write', path: 'b(1).md', reasoning: 'r', ok: true, message: 'Successfully wrote 12 bytes to b(1).md' },
        { type: 'edit', path: 'c.ts', reasoning: 'r', ok: false, message: 'Could not find the exact text in c.ts.' },
      ]),
    ).toBe('1. OK edit src/a.ts: replaced 2 block(s)\n2. OK write b(1).md: wrote 12 bytes\n3. FAILED edit c.ts: Could not find the exact text in c.ts.');
  });
});

describe('file change rendering', () => {

  it('shows each file change with its reasoning, size, outcome and a capped diff', () => {
    initTheme('dark');
    let tool: { renderCall: Function; renderResult: Function } | undefined;
    registerFileTool({ registerTool: (definition: never) => (tool = definition), on: () => undefined } as never, new FileGuard(), fakePermission());
    const args = {
      queries: [
        { reasoning: 'Fix the off-by-one in the loop', type: 'edit', path: 'src/a.ts', edits: [{ oldText: 'a', newText: 'b' }, { oldText: 'c', newText: 'd' }] },
        { reasoning: 'Add the missing test', type: 'write', path: 'test/a.test.ts', content: 'one\ntwo' },
      ],
    };
    const call = tool!.renderCall(args, theme, { lastComponent: undefined, isPartial: true, executionStarted: false }).render(120).join('\n');
    expect(call.split('\n')[0]).toBe('○ File(2 changes)');
    expect(call).toMatch(/edit src\/a\.ts · 2 edits/);
    expect(call).toMatch(/write test\/a\.test\.ts · 2 lines/);
    // Reasoning shows only when expanded.
    expect(call).not.toContain('↳');
    const expandedCall = tool!.renderCall(args, theme, { lastComponent: undefined, expanded: true }).render(120).join('\n');
    expect(expandedCall).toContain('↳ Fix the off-by-one in the loop');
    expect(expandedCall).toContain('↳ Add the missing test');
    const single = tool!.renderCall({ queries: [args.queries[0]] }, theme, { lastComponent: undefined, isPartial: false, isError: false, durationMs: 1200 }).render(120).join('\n');
    expect(single).toBe('● File(edit src/a.ts · 2 edits) · 1.2s');
    const diff = Array.from({ length: 30 }, (_, index) => `+${index} added line ${index}`).join('\n');
    const result = { content: [], details: { outcomes: [{ type: 'edit', path: 'src/a.ts', reasoning: '', ok: true, message: 'ok', diff }, { type: 'write', path: 'test/a.test.ts', reasoning: '', ok: false, message: 'has not been read' }] } };
    const collapsed = tool!.renderResult(result, { expanded: false, isPartial: false }, theme, { lastComponent: undefined, isError: false }).render(120).join('\n');
    expect(collapsed.split('\n')[0]).toBe('  ⎿  Applied 1 of 2 changes');
    expect(collapsed).toMatch(/✓ edit src\/a\.ts \+30 -0/);
    expect(collapsed).toMatch(/… \+\d+ diff lines \(ctrl\+o to expand\)/);
    expect(collapsed).toMatch(/✗ write test\/a\.test\.ts: has not been read/);
    expect(collapsed).not.toContain('added line');
    const one = { content: [], details: { outcomes: [result.details.outcomes[0]] } };
    const oneCollapsed = tool!.renderResult(one, { expanded: false, isPartial: false }, theme, { lastComponent: undefined, isError: false, durationMs: 5 }).render(120).join('\n').split('\n');
    expect(oneCollapsed[0]).toMatch(/^ {2}⎿ {2}✓ edit src\/a\.ts \+30 -0$/);
    expect(oneCollapsed).toHaveLength(1 + 6 + 1);
    expect(oneCollapsed.at(-1)).toMatch(/… \+\d+ lines \(ctrl\+o to expand\)/);
    const failed = tool!.renderResult({ content: [{ type: 'text', text: '1. FAILED edit src/a.ts: oldText not found' }] }, { expanded: false, isPartial: false }, theme, { lastComponent: undefined, isError: true }).render(120).join('\n');
    expect(failed).toBe('  ⎿  Error: edit src/a.ts: oldText not found');
    const expanded = tool!.renderResult(result, { expanded: true, isPartial: false }, theme, { lastComponent: undefined, isError: false }).render(120).join('\n');
    expect(expanded).toContain('added line 29');
    expect(expanded).not.toMatch(/diff lines/);
  });

  it('counts diff lines and sizes queries', () => {
    expect(diffStats('+1 a\n-2 b\n-3 c\n 4 d')).toEqual({ added: 1, removed: 2 });
    expect(querySize({ type: 'edit', edits: [{}] })).toBe(' · 1 edit');
    expect(querySize({ type: 'delete' })).toBe('');
  });
});
