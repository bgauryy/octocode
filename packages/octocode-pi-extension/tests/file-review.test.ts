import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { FileGuard, registerFileTool } from '../src/files/tool.js';
import { describeChange, reviewChanges } from '../src/files/review.js';
import { tmp } from './helpers.js';


function ui(answers: { select?: string | undefined; confirms?: boolean[] }) {
  const confirms = [...(answers.confirms ?? [])];
  const asked: string[] = [];
  return {
    asked,
    ctx: { ui: { select: async (title: string) => (asked.push(title), answers.select), confirm: async (title: string, message: string) => (asked.push(`${title}\n${message}`), confirms.shift() ?? false) } } as never,
  };
}

const changes = [
  { type: 'edit', path: 'src/a.ts', reasoning: 'Fix the loop', edits: [{ oldText: 'i <= n', newText: 'i < n' }] },
  { type: 'write', path: 'src/new.ts', reasoning: 'Add helper', content: 'export const x = 1;\nexport const y = 2;' },
  { type: 'delete', path: 'old.ts', reasoning: 'Unused' },
];

describe('file change review', () => {
  it('describes each change with its reasoning and a bounded preview', () => {
    expect(describeChange(changes[0]!)).toEqual(['edit src/a.ts · 1 edit', '  why: Fix the loop', '  - i <= n', '  + i < n']);
    expect(describeChange(changes[1]!).join('\n')).toContain('+ export const x = 1;');
    expect(describeChange(changes[2]!)).toContain('  removes the file');
    expect(describeChange({ type: 'write', path: 'p', reasoning: 'r', content: 'one\n' })).toEqual(['write p', '  why: r', '  + one']);
    const long = describeChange({ type: 'write', path: 'p', reasoning: 'r', content: Array.from({ length: 20 }, (_, i) => `line ${i}`).join('\n') });
    expect(long.join('\n')).toContain('… 15 more line(s)');
  });

  it('applies all, rejects all, or decides one by one', async () => {
    const all = ui({ select: 'Apply all' });
    expect([...(await reviewChanges(all.ctx, changes))]).toEqual([]);
    expect(all.asked[0]).toContain('1. edit src/a.ts — Fix the loop');
    expect(all.asked[0]).toContain('3. delete old.ts — Unused');
    expect(((await reviewChanges(ui({ select: 'Reject all' }).ctx, changes)) as Set<number>).size).toBe(3);
    expect(await reviewChanges(ui({ select: undefined }).ctx, changes)).toBe('cancelled'); // dismissed: no verdict
    const aborted = new AbortController();
    const midway = ui({ select: 'Review one by one', confirms: [true, false, true] });
    const confirm = (midway.ctx as { ui: { confirm: Function } }).ui.confirm;
    (midway.ctx as { ui: { confirm: Function } }).ui.confirm = async (...args: unknown[]) => (aborted.abort(), confirm(...args));
    expect(await reviewChanges(midway.ctx, changes, aborted.signal)).toBe('cancelled');
    const each = ui({ select: 'Review one by one', confirms: [true, false, true] });
    expect([...(await reviewChanges(each.ctx, changes))]).toEqual([1]);
    expect(each.asked[2]).toContain('Change 2 of 3');
    expect(each.asked[2]).toContain('why: Add helper');
  });

  it('applies only the accepted changes of a batch and tells the model about the rest', async () => {
    const cwd = tmp();
    fs.writeFileSync(path.join(cwd, 'a.ts'), 'const a = 1;\n');
    let tool: { execute: Function } | undefined;
    const review = { on: true };
    registerFileTool({ registerTool: (definition: never) => (tool = definition), on: () => undefined } as never, new FileGuard(), review);
    const batch = { queries: [{ type: 'edit', path: 'a.ts', reasoning: 'bump', edits: [{ oldText: '1', newText: '2' }] }, { type: 'write', path: 'b.ts', reasoning: 'add', content: 'x' }] };
    const dialog = ui({ select: 'Review one by one', confirms: [true, false] });
    const ctx = { cwd, hasUI: true, ui: (dialog.ctx as { ui: unknown }).ui };
    const result = await tool!.execute('1', batch, undefined, undefined, ctx);
    expect(result.content[0].text).toContain('1. OK edit a.ts');
    expect(result.content[0].text).toContain('2. FAILED write b.ts: Rejected by the user');
    expect(fs.readFileSync(path.join(cwd, 'a.ts'), 'utf8')).toBe('const a = 2;\n');
    expect(fs.existsSync(path.join(cwd, 'b.ts'))).toBe(false);
    // Rejecting everything is an error the model must not retry around; off skips the dialog entirely.
    await expect(tool!.execute('2', batch, undefined, undefined, { ...ctx, ui: (ui({ select: 'Reject all' }).ctx as { ui: unknown }).ui })).rejects.toThrow(/rejected/);
    await expect(tool!.execute('2b', batch, undefined, undefined, { ...ctx, ui: (ui({ select: undefined }).ctx as { ui: unknown }).ui })).rejects.toThrow(/^Cancelled before any change was applied/);
    review.on = false;
    const silent = ui({});
    await tool!.execute('3', { queries: [{ type: 'write', path: 'c.ts', reasoning: 'add', content: 'y' }] }, undefined, undefined, { ...ctx, ui: (silent.ctx as { ui: unknown }).ui });
    expect(silent.asked).toEqual([]);
    expect(fs.existsSync(path.join(cwd, 'c.ts'))).toBe(true);
  });
});
