import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { FileGuard } from '../src/files/guard.js';
import { registerFileTool } from '../src/files/tool.js';
import { askReview, describeChange } from '../src/files/review.js';
import { fakeCtx, fakePermission, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';


function ui(answers: { selects?: Array<string | undefined>; inputs?: Array<string | undefined>; onSelect?: () => void }) {
  const selects = [...(answers.selects ?? [])];
  const inputs = [...(answers.inputs ?? [])];
  const asked: string[] = [];
  return {
    asked,
    ctx: { ui: { select: async (title: string) => (asked.push(title), answers.onSelect?.(), selects.shift()), input: async () => inputs.shift() } } as never,
  };
}

const changes = [
  { type: 'edit', path: 'src/a.ts', reasoning: 'Fix the loop', edits: [{ oldText: 'i <= n', newText: 'i < n' }] },
  { type: 'write', path: 'src/new.ts', reasoning: 'Add helper', content: 'export const x = 1;\nexport const y = 2;' },
  { type: 'delete', path: 'old.ts', reasoning: 'Unused' },
];

describe('file change review', () => {
  it('describes each change with its reasoning and a bounded preview', () => {
    expect(describeChange(changes[0]!)).toEqual(['edit src/a.ts · 1 edit', '  - i <= n', '  + i < n', '  agent says (unverified): Fix the loop']);
    expect(describeChange(changes[1]!).join('\n')).toContain('+ export const x = 1;');
    expect(describeChange(changes[2]!)).toEqual(['delete old.ts', '  removes the file', '  agent says (unverified): Unused']);
    expect(describeChange({ type: 'write', path: 'p', reasoning: 'r', content: 'one\n' })).toEqual(['write p', '  + one', '  agent says (unverified): r']);
    const long = describeChange({ type: 'write', path: 'p', reasoning: 'r', content: Array.from({ length: 20 }, (_, i) => `line ${i}`).join('\n') });
    expect(long.join('\n')).toContain('… 15 more line(s)');
  });

  it('applies all, rejects all, or decides one by one', async () => {
    const all = ui({ selects: ['Apply all'] });
    expect([...(await askReview(all.ctx, changes))]).toEqual([]);
    expect(all.asked[0]).toContain('1. edit src/a.ts\n2. write src/new.ts\n3. delete old.ts');
    // What the agent says comes last, per change and labelled unverified.
    expect(all.asked[0].split('\n').slice(-3)).toEqual(['  1. agent says (unverified): Fix the loop', '  2. agent says (unverified): Add helper', '  3. agent says (unverified): Unused']);
    const withReasons = ui({ selects: ['Apply all'] });
    await askReview(withReasons.ctx, changes, undefined, ['▸ why it asks: it changes files in the repository (manual mode asks)']);
    expect(withReasons.asked[0].indexOf('▸ why it asks')).toBeLessThan(withReasons.asked[0].indexOf('agent says'));
    expect(((await askReview(ui({ selects: ['Reject all'] }).ctx, changes)) as Map<number, string | undefined>).size).toBe(3);
    expect(await askReview(ui({}).ctx, changes)).toBe('cancelled'); // dismissed: no verdict
    const aborted = new AbortController();
    let calls = 0;
    const midway = ui({ selects: ['Review one by one', 'Apply'], onSelect: () => { if (++calls === 2) aborted.abort(); } });
    expect(await askReview(midway.ctx, changes, aborted.signal)).toBe('cancelled');
    const each = ui({ selects: ['Review one by one', 'Apply', 'Reject', 'Apply'] });
    expect([...(await askReview(each.ctx, changes))]).toEqual([[1, undefined]]);
    expect(each.asked[2]).toContain('Change 2 of 3');
    expect(each.asked[2]).toContain('agent says (unverified): Add helper');
  });

  it('keeps rejection notes for one file, the whole batch, and individual changes', async () => {
    for (const batch of [[changes[0]!], changes]) {
      const reject = ui({ selects: ['Reject all and tell the agent why…'], inputs: ['  Keep the existing API  '] });
      expect(await askReview(reject.ctx, batch)).toEqual(new Map(batch.map((_, index) => [index, 'Keep the existing API'])));
    }
    const each = ui({ selects: ['Review one by one', 'Reject and tell the agent why…', 'Apply', 'Reject'], inputs: ['Fix the condition instead'] });
    expect(await askReview(each.ctx, changes)).toEqual(new Map([[0, 'Fix the condition instead'], [2, undefined]]));
    const empty = ui({ selects: ['Reject all and tell the agent why…'], inputs: ['  '] });
    expect(await askReview(empty.ctx, [changes[0]!])).toEqual(new Map([[0, undefined]]));
  });

  it('cancels without a verdict when a review or rejection note is dismissed', async () => {
    const cases = [
      { selects: ['Reject all and tell the agent why…'], inputs: [undefined] },
      { selects: ['Review one by one', 'Apply', undefined] },
      { selects: ['Review one by one', 'Apply', 'Reject and tell the agent why…'], inputs: [undefined] },
      { selects: ['Unrecognized reply'] },
    ];
    for (const answers of cases) expect(await askReview(ui(answers).ctx, changes)).toBe('cancelled');
  });

  it('closes a revoked file review without writing and lets the next review open', async () => {
    const cwd = tmp();
    const fake = fakePi();
    const permission = fakePermission('manual');
    registerFileTool(fake.pi, new FileGuard(), permission);
    const tool = fake.tools.get('file')!;
    const ctx = fakeCtx({ cwd });
    let reviewSignal: AbortSignal | undefined;
    let dismiss!: () => void;
    let open = false;
    ctx.ui.select = async (_title, _choices, options) => {
      reviewSignal = options?.signal;
      open = true;
      return new Promise<string | undefined>((resolve) => {
        dismiss = () => { open = false; resolve(undefined); };
        reviewSignal?.addEventListener('abort', dismiss, { once: true });
      });
    };
    const params = { queries: [{ type: 'write', path: 'new.ts', reasoning: 'add', content: 'new' }] };
    const pending = tool.execute('revoked', params, undefined, undefined, ctx);
    const cancelled = expect(pending).rejects.toThrow(/^Cancelled before any change was applied/);
    await new Promise((resolve) => setImmediate(resolve));
    permission.clearGrants();
    try {
      expect(reviewSignal?.aborted).toBe(true);
      expect(open).toBe(false);
    } finally {
      dismiss();
      await cancelled;
    }
    expect(fs.existsSync(path.join(cwd, 'new.ts'))).toBe(false);
    ctx.ui.select = async () => 'Apply all';
    await tool.execute('fresh', params, undefined, undefined, ctx);
    expect(fs.readFileSync(path.join(cwd, 'new.ts'), 'utf8')).toBe('new');
  });

  it('is the manual mode\'s file ask: applies only the accepted changes of a batch and tells the model about the rest', async () => {
    const cwd = tmp();
    fs.writeFileSync(path.join(cwd, 'a.ts'), 'const a = 1;\n');
    let tool: { execute: Function } | undefined;
    const permission = fakePermission('manual');
    const guard = new FileGuard();
    guard.record(path.join(cwd, 'a.ts'));
    registerFileTool({ registerTool: (definition: never) => (tool = definition), on: () => undefined } as never, guard, permission);
    const batch = { queries: [{ type: 'edit', path: 'a.ts', reasoning: 'bump', edits: [{ oldText: '1', newText: '2' }] }, { type: 'write', path: 'b.ts', reasoning: 'add', content: 'x' }] };
    const dialog = ui({ selects: ['Review one by one', 'Apply', 'Reject and tell the agent why…'], inputs: ['Use the existing helper'] });
    const ctx = { cwd, hasUI: true, ui: (dialog.ctx as { ui: unknown }).ui };
    const result = await tool!.execute('1', batch, undefined, undefined, ctx);
    expect(result.content[0].text).toContain('1. OK edit a.ts');
    expect(result.content[0].text).toContain('2. FAILED write b.ts: Rejected by the user');
    expect(result.content[0].text).toContain("User's note: Use the existing helper");
    expect(fs.readFileSync(path.join(cwd, 'a.ts'), 'utf8')).toBe('const a = 2;\n');
    expect(fs.existsSync(path.join(cwd, 'b.ts'))).toBe(false);
    // Rejecting everything is an error the model must not retry around; workspace mode skips the dialog entirely.
    await expect(tool!.execute('2', batch, undefined, undefined, { ...ctx, ui: (ui({ selects: ['Reject all'] }).ctx as { ui: unknown }).ui })).rejects.toThrow(/denied/);
    await expect(tool!.execute('2a', batch, undefined, undefined, { ...ctx, ui: (ui({ selects: ['Reject all and tell the agent why…'], inputs: ['Change the tests first'] }).ctx as { ui: unknown }).ui })).rejects.toThrow(/User's note:.*Change the tests first/s);
    await expect(tool!.execute('2b', batch, undefined, undefined, { ...ctx, ui: (ui({}).ctx as { ui: unknown }).ui })).rejects.toThrow(/^Cancelled before any change was applied/);
    const cancelledBatch = { queries: ['cancel-a.ts', 'cancel-b.ts'].map((file) => ({ type: 'write', path: file, reasoning: 'add', content: 'new' })) };
    const cancelledReview = ui({ selects: ['Review one by one', 'Apply', 'Reject and tell the agent why…'], inputs: [undefined] });
    await expect(tool!.execute('cancel', cancelledBatch, undefined, undefined, { ...ctx, ui: (cancelledReview.ctx as { ui: unknown }).ui })).rejects.toThrow(/^Cancelled before any change was applied/);
    for (const query of cancelledBatch.queries) expect(fs.existsSync(path.join(cwd, query.path))).toBe(false);
    // Without a UI the manual ask is refused: nothing is applied unasked.
    await expect(tool!.execute('2c', batch, undefined, undefined, { ...ctx, hasUI: false })).rejects.toThrow(/the file change .*needs the user's approval \(manual mode\) and no one can be asked here/);
    expect(fs.existsSync(path.join(cwd, 'b.ts'))).toBe(false);
    permission.setMode('workspace', fakeCtx({ cwd }));
    const silent = ui({});
    await tool!.execute('3', { queries: [{ type: 'write', path: 'c.ts', reasoning: 'add', content: 'y' }] }, undefined, undefined, { ...ctx, ui: (silent.ctx as { ui: unknown }).ui });
    expect(silent.asked).toEqual([]);
    expect(fs.existsSync(path.join(cwd, 'c.ts'))).toBe(true);
  });
});
