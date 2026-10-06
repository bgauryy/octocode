import { describe, expect, it } from 'vitest';
import { showAskDialog, type AskQuestion, type AskResult } from '../src/ask/dialog.js';
import { formatAnswers, registerAskUser } from '../src/ask/tool.js';
import { fakeCtx, fakePi, rendered, theme } from './fake-pi.js';
import { visibleWidth } from '@earendil-works/pi-tui';

const KEY = { enter: '\r', escape: '\x1b', tab: '\t', shiftTab: '\x1b[Z', up: '\x1b[A', down: '\x1b[B', right: '\x1b[C', left: '\x1b[D', space: ' ' };
const tui = { requestRender: () => undefined, terminal: { rows: 40, columns: 100 } };

type Dialog = { render(width: number): string[]; handleInput(data: string): void; invalidate(): void; dispose(): void; focused: boolean };

/** Open the dialog in a fake TUI, feed it keys, and return the result plus the frames seen after each key. */
async function drive(questions: AskQuestion[], keys: string[], signal?: AbortSignal): Promise<{ result: AskResult; frames: string[] }> {
  const frames: string[] = [];
  const ctx = fakeCtx({
    cwd: process.cwd(),
    ui: {
      custom: (factory) =>
        new Promise((resolve) => {
          const dialog = factory(tui, theme, {}, resolve) as Dialog;
          dialog.focused = true;
          expect(dialog.focused).toBe(true);
          frames.push(dialog.render(80).join('\n'));
          for (const key of keys) {
            dialog.handleInput(key);
            frames.push(dialog.render(80).join('\n'));
          }
          dialog.invalidate();
        }),
    },
  });
  const result = (await showAskDialog(ctx, questions, signal)) as AskResult;
  return { result, frames };
}

const scope: AskQuestion = { question: 'Which scope?', header: 'Scope', options: [{ label: 'Small (Recommended)', description: 'One file' }, { label: 'Large' }] };
const extras: AskQuestion = { question: 'Which extras?', header: 'Extras', multiSelect: true, options: [{ label: 'Docs' }, { label: 'Tests' }, { label: 'Bench' }] };

describe('askUser', () => {
  it('rewraps on resize and strips terminal controls from question text', async () => {
    const ctx = fakeCtx({ cwd: '.', ui: { custom: (factory) => new Promise((resolve) => {
      const dialog = factory(tui, theme, {}, resolve) as Dialog;
      const wide = dialog.render(80);
      const narrow = dialog.render(12);
      expect(narrow).not.toEqual(wide);
      expect(narrow.every((line) => visibleWidth(line) <= 12)).toBe(true);
      expect(narrow.join('\n')).not.toContain('\u001b]0;hidden');
      dialog.handleInput(KEY.escape);
    }) } });
    await showAskDialog(ctx, [{ ...scope, question: 'Which\u001b]0;hidden\u0007 scope?' }]);
  });

  it('formats askUser answers and declines', () => {
    expect(formatAnswers({ cancelled: false, answers: [{ question: 'Which?', answer: 'A', custom: false }] })).toBe('"Which?" → A');
    expect(formatAnswers({ cancelled: true, answers: [] })).toMatch(/declined/);
    const partial = formatAnswers({ cancelled: true, answers: [{ question: 'Which?', answer: 'A', custom: false }] });
    expect(partial).toMatch(/^"Which\?" → A\n/);
    expect(partial).toMatch(/declined to answer the remaining questions/);
    expect(formatAnswers({ cancelled: false, answers: [{ question: 'Q?', answer: 'mine', custom: true }] })).toBe('"Q?" → (typed) mine');
    expect(formatAnswers({ cancelled: false, answers: [{ question: 'Q?', answer: 'A, B, mine', custom: true, picks: ['A', 'B'], typed: 'mine' }] })).toBe('"Q?" → A, B + (typed) mine');
  });

  it('picks a single answer with the arrows and Enter', async () => {
    const { result, frames } = await drive([scope], [KEY.down, KEY.up, KEY.up, KEY.down, KEY.down, KEY.enter]);
    expect(result).toEqual({ cancelled: false, answers: [{ question: 'Which scope?', answer: 'Large', custom: false }] });
    expect(frames[0]).toContain('Which scope?');
    expect(frames[0]).toContain('› 1. Small (Recommended)');
    expect(frames[0]).toContain('One file');
    expect(frames[0]).toContain('3. Write an answer');
    expect(frames[0]).toContain('↑↓/1-9 move · Enter choose · Esc cancel');
    expect(frames[0]).toContain('Choose one');
    expect(frames[0]).not.toContain('Submit');
  });

  it('jumps with number keys, types a custom answer, and backs out of typing with Esc', async () => {
    const typed = await drive([scope], ['9', '3', KEY.enter, 'x', KEY.escape, KEY.enter, ...' my own', KEY.enter]);
    expect(typed.result).toEqual({ cancelled: false, answers: [{ question: 'Which scope?', answer: 'x my own', custom: true }] });
    expect(typed.frames[3]).toContain('Enter submit · Esc back');
    // An empty submission keeps the dialog open and leaves typing mode.
    const empty = await drive([scope], ['3', KEY.enter, KEY.enter, KEY.escape]);
    expect(empty.frames[3]).toContain('Enter choose');
    const cancelled = await drive([scope], ['q', KEY.escape]);
    expect(cancelled.result).toEqual({ cancelled: true, answers: [] });
  });

  it('opens the editor for a free-text question reached by Tab, with its draft', async () => {
    const { result, frames } = await drive([scope, { question: 'Why?', header: 'Why', options: [] }], [KEY.enter, ...'abc', KEY.escape, KEY.shiftTab, KEY.tab, KEY.enter, KEY.enter]);
    expect(frames[7]).toContain('Enter submit · Esc back');
    expect(frames[7]).toContain('abc');
    expect(result.answers).toEqual([{ question: 'Which scope?', answer: 'Small (Recommended)', custom: false }, { question: 'Why?', answer: 'abc', custom: true }]);
  });

  it('opens the editor directly for free-text questions', async () => {
    const { result, frames } = await drive([{ question: 'Which region?', options: [] }], [...'eu-west-1', KEY.enter]);
    expect(frames[0]).toContain('Enter submit');
    expect(result.answers).toEqual([{ question: 'Which region?', answer: 'eu-west-1', custom: true }]);
  });

  it('toggles several options, adds typed text, and submits a multi-question dialog', async () => {
    const keys = [
      KEY.enter, // Scope: Small → moves to Extras
      KEY.space, KEY.down, KEY.down, KEY.space, KEY.space, KEY.space, // toggle Docs and Bench (Bench twice off, then on)
      '5', KEY.enter, // Submit selection row: Extras → Docs, Bench → Submit tab
      KEY.left, KEY.right, KEY.tab, KEY.shiftTab, // wander between tabs and back to Submit
      KEY.enter,
    ];
    const { result, frames } = await drive([scope, extras], keys);
    expect(result).toEqual({
      cancelled: false,
      answers: [
        { question: 'Which scope?', answer: 'Small (Recommended)', custom: false },
        { question: 'Which extras?', answer: 'Docs, Bench', custom: false },
      ],
    });
    expect(frames[0]).toContain('□ Scope');
    expect(frames[0]).toContain('✓ Submit');
    expect(frames[2]).toContain('[x] Docs');
    expect(frames[2]).toContain('Space/Enter toggle');
    expect(frames[2]).toContain('Choose one or more, then Submit selection');
    expect(frames[7]).toContain('✓ Submit selection (2 selected)');
    expect(frames[9]).toContain('Scope: Small (Recommended)');
    expect(frames[9]).toContain('Enter to submit');

    // Submit refuses until every question is answered; typed text joins the toggled options.
    const typed = await drive([scope, extras], [KEY.tab, KEY.tab, KEY.enter, KEY.shiftTab, KEY.space, '4', KEY.enter, ...'CI', KEY.enter, KEY.tab, KEY.enter, KEY.tab, KEY.enter]);
    expect(typed.frames[3]).toContain('Answer every question to submit');
    expect(typed.frames[3]).toContain('(unanswered)');
    expect(typed.result).toEqual({
      cancelled: false,
      answers: [
        { question: 'Which scope?', answer: 'Small (Recommended)', custom: false },
        { question: 'Which extras?', answer: 'Docs, CI', custom: true, picks: ['Docs'], typed: 'CI' },
      ],
    });

    // Multi-select: Enter toggles instead of answering; Submit selection needs at least one pick.
    const plain = await drive([extras], [KEY.down, KEY.enter, KEY.enter, '5', KEY.enter, '1', KEY.enter, '3', KEY.enter, '5', KEY.enter]);
    expect(plain.frames[2]).toContain('[x] Tests');
    expect(plain.frames[3]).toContain('[ ] Tests');
    expect(plain.frames[5]).toContain('Select at least one option');
    expect(plain.frames[6]).not.toContain('Select at least one option');
    expect(plain.result).toEqual({ cancelled: false, answers: [{ question: 'Which extras?', answer: 'Docs, Bench', custom: false }] });
  });

  it('closes as cancelled when the turn is aborted', async () => {
    const controller = new AbortController();
    controller.abort();
    const early = await drive([scope], [], controller.signal);
    expect(early.result).toEqual({ cancelled: true, reason: 'aborted', answers: [] });

    const later = new AbortController();
    const pending = drive([scope, extras], [KEY.enter], later.signal);
    later.abort();
    const { result } = await pending;
    expect(result).toEqual({ cancelled: true, reason: 'aborted', answers: [{ question: 'Which scope?', answer: 'Small (Recommended)', custom: false }] });
  });
});

describe('askUser tool', () => {
  const fake = fakePi();
  registerAskUser(fake.pi);
  const tool = fake.tools.get('askUser');
  const params = { questions: [{ ...scope, header: 'A very long header' }, extras] };

  it('answers for a missing user when there is no UI', async () => {
    const result = await tool.execute('c', params, undefined, undefined, fakeCtx({ cwd: '.', hasUI: false }));
    expect(result.content[0].text).toMatch(/No interactive user/);
    expect(result.details).toMatchObject({ answers: [], cancelled: true });
    expect(result.content[0].text).not.toContain('Proceed with the recommended option');
    expect(result.content[0].text).toContain('approval');
    expect(rendered(tool.renderResult(result, {}, theme, {}))).toContain('Unavailable');
  });

  it('accepts a free-text question without manufactured choices and respects interruption', async () => {
    const params = { questions: [{ question: 'What is the deployment region?' }] };
    const ctx = fakeCtx({ cwd: '.', mode: 'rpc', ui: { inputs: ['eu-west-1'] } });
    const result = await tool.execute('c', params, undefined, undefined, ctx);
    expect(result.details).toMatchObject({ cancelled: false, answers: [{ answer: 'eu-west-1', custom: true }] });
    const controller = new AbortController();
    controller.abort();
    const aborted = await tool.execute('c', params, controller.signal, undefined, ctx);
    expect(aborted.details).toMatchObject({ cancelled: true, reason: 'aborted', answers: [] });
  });

  it('falls back to select and input dialogs in RPC mode', async () => {
    const ctx = fakeCtx({ cwd: '.', mode: 'rpc', ui: { selects: ['2. Large', 'Write an answer'], inputs: ['  just docs  '] } });
    const result = await tool.execute('c', params, undefined, undefined, ctx);
    expect(result.details).toMatchObject({
      cancelled: false,
      answers: [
        { question: 'Which scope?', answer: 'Large', custom: false },
        { question: 'Which extras?', answer: 'just docs', custom: true },
      ],
    });
    // Labels with a description map back to the bare label.
    const described = await tool.execute('c', params, undefined, undefined, fakeCtx({ cwd: '.', mode: 'rpc', ui: { selects: ['1. Small (Recommended) — One file', undefined] } }));
    expect(described.details).toMatchObject({ cancelled: true, answers: [{ question: 'Which scope?', answer: 'Small (Recommended)', custom: false }] });
    const emptyTyped = await tool.execute('c', params, undefined, undefined, fakeCtx({ cwd: '.', mode: 'rpc', ui: { selects: ['Write an answer'], inputs: ['   '] } }));
    expect(emptyTyped.details).toMatchObject({ cancelled: true, answers: [] });
    // Multi-select in RPC: one pick per round until Done, every option, or typed text.
    const multi = { questions: [extras] };
    const done = await tool.execute('c', multi, undefined, undefined, fakeCtx({ cwd: '.', mode: 'rpc', ui: { selects: ['3. Bench', '1. Docs', 'Submit selection'] } }));
    expect(done.details).toMatchObject({ cancelled: false, answers: [{ question: 'Which extras?', answer: 'Docs, Bench', custom: false }] });
    const all = await tool.execute('c', multi, undefined, undefined, fakeCtx({ cwd: '.', mode: 'rpc', ui: { selects: ['1. Docs', '2. Tests', '3. Bench'] } }));
    expect(all.details.answers[0].answer).toBe('Docs, Tests, Bench');
    const typed = await tool.execute('c', multi, undefined, undefined, fakeCtx({ cwd: '.', mode: 'rpc', ui: { selects: ['2. Tests', 'Write an answer'], inputs: ['CI'] } }));
    expect(typed.details.answers[0]).toEqual({ question: 'Which extras?', answer: 'Tests, CI', custom: true, picks: ['Tests'], typed: 'CI' });
    expect(typed.content[0].text).toBe('"Which extras?" → Tests + (typed) CI');
  });

  it('distinguishes duplicate labels and choices named after UI actions in RPC mode', async () => {
    const questions = [{ question: 'Which?', options: [{ label: 'Done.', description: 'first' }, { label: 'Done.', description: 'second' }, { label: 'Write an answer' }] }];
    const ctx = fakeCtx({ cwd: '.', mode: 'rpc', ui: { selects: ['2. Done. — second'] } });
    const result = await tool.execute('c', { questions }, undefined, undefined, ctx);
    expect(result.details).toMatchObject({ cancelled: false, answers: [{ answer: 'Done.', custom: false }] });
  });

  it('opens the inline dialog in the TUI with headers clipped to 12 characters', async () => {
    let headerSeen = '';
    const ctx = fakeCtx({
      cwd: '.',
      ui: {
        custom: (factory) =>
          new Promise((resolve) => {
            const dialog = factory(tui, theme, {}, resolve) as Dialog;
            headerSeen = dialog.render(80).join('\n');
            dialog.handleInput(KEY.escape);
            dialog.dispose();
          }),
      },
    });
    const result = await tool.execute('c', params, undefined, undefined, ctx);
    expect(headerSeen).toContain('A very long…');
    expect(headerSeen).not.toContain('A very long header');
    expect(result.content[0].text).toMatch(/declined/);
  });

  it('renders the call and the answers', () => {
    expect(rendered(tool.renderCall(params, theme, {}))).toBe('○ Ask(A very long header, Extras)');
    expect(rendered(tool.renderCall({ questions: [{ header: 'bad\u001b[31m\nnext' }] }, theme, {}))).toBe('○ Ask(bad)');
    expect(rendered(tool.renderCall({}, theme, {}))).toBe('○ Ask');
    expect(rendered(tool.renderResult({ content: [] }, {}, theme, {}))).toBe('');
    const answered = { content: [], details: { cancelled: true, answers: [{ question: 'Which scope?', answer: 'Large\u001b]0;x\u0007', custom: false }] } };
    expect(rendered(tool.renderResult(answered, {}, theme, {}))).toBe('  ⎿  ✓ Which scope? → Large\n     Declined the remaining questions');
    expect(rendered(tool.renderResult({ content: [], details: { cancelled: true, answers: [] } }, {}, theme, {}))).toBe('  ⎿  Declined');
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'No interactive user' }] }, {}, theme, {}))).toBe('  ⎿  No interactive user');
  });
});
