import { describe, expect, it } from 'vitest';
import { NEEDS_APPROVAL } from '../src/permission/dialog.js';
import { newChildState, reduceEvent, type ChildEffect } from '../src/subagents/events.js';

const LIMITS = { idleMs: 1_000, busyMs: 5_000 };
const NOW = 100_000;

const reduce = (state: ReturnType<typeof newChildState>, event: Record<string, unknown>, now = NOW): ChildEffect[] => reduceEvent(state, event, LIMITS, now);
const progress = (effects: ChildEffect[]) => effects.flatMap((effect) => (effect.kind === 'progress' ? [effect.update] : []));
const reply = (stopReason: string, text = 'answer') => ({ type: 'message_end', message: { role: 'assistant', stopReason, content: [{ type: 'text', text }], usage: { input: 3, output: 2, cacheRead: 0, cacheWrite: 0, totalTokens: 5, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } } });

describe('reduceEvent', () => {
  it('marks the child running at agent_start and settled at agent_settled, and reports the child\'s own abort flag', () => {
    const state = newChildState();
    expect(reduce(state, { type: 'agent_start' })).toEqual([]);
    expect(state.running).toBe(true);
    expect(reduce(state, { type: 'agent_settled', aborted: true })).toEqual([{ kind: 'settled', aborted: true }]);
    expect(state).toMatchObject({ running: false, settledSeen: true });
    expect(reduce(newChildState(), { type: 'agent_settled' })).toEqual([{ kind: 'settled', aborted: false }]);
  });

  it('starts a tool call: counts it, sets its busy deadline from its own timeout, and reports it as activity', () => {
    const state = newChildState();
    const effects = reduce(state, { type: 'tool_execution_start', toolName: 'bash', args: { command: 'sleep 9', timeout: 20 } });
    expect(state.toolsRunning).toBe(1);
    expect(state.busyUntil).toBe(NOW + 20_000 + LIMITS.idleMs);
    expect(state.lastActivity).toBe('→ bash sleep 9');
    expect(progress(effects)).toEqual([{ activity: '→ bash sleep 9', usage: state.usage }]);
    // A call without a timeout gets the default busy time, and never shortens an earlier deadline.
    reduce(state, { type: 'tool_execution_start', toolName: 'read', args: { path: 'a' } }, NOW + 1);
    expect(state.toolsRunning).toBe(2);
    expect(state.busyUntil).toBe(NOW + 20_000 + LIMITS.idleMs);
  });

  it('keeps the longest deadline of a background job for settling', () => {
    const state = newChildState();
    reduce(state, { type: 'tool_execution_start', toolName: 'bash', args: { command: 'x', timeout: 60, background: true } });
    reduce(state, { type: 'tool_execution_start', toolName: 'bash', args: { command: 'y', timeout: 30, background: true } });
    expect(state.jobsMs).toBe(60_000);
    reduce(state, reply('stop'), NOW + 5);
    expect(state.busyUntil).toBe(NOW + 5 + 60_000 + LIMITS.idleMs);
  });

  it('ends a tool call and reports its screenshot and a refusal that needs approval', () => {
    const state = newChildState();
    reduce(state, { type: 'tool_execution_start', toolName: 'browser', args: {} });
    const effects = reduce(state, { type: 'tool_execution_end', isError: true, result: { content: [{ type: 'image', data: 'AAA', mimeType: 'image/jpeg' }, { type: 'text', text: 'failed' }] } });
    expect(state.toolsRunning).toBe(0);
    expect(progress(effects)).toEqual([{ image: { data: 'AAA', mimeType: 'image/jpeg' }, usage: state.usage }]);
    const denied = reduce(state, { type: 'tool_execution_end', isError: true, result: { content: [{ type: 'text', text: `${NEEDS_APPROVAL} bash` }] } });
    expect(progress(denied)).toEqual([{ denied: true, usage: state.usage }]);
    // An end without a start never drives the count below zero.
    expect(state.toolsRunning).toBe(0);
  });

  it('clears running tools at a new turn or a streaming reply, and ends settling at a turn', () => {
    const state = newChildState();
    reduce(state, { type: 'tool_execution_start', toolName: 'bash', args: {} });
    reduce(state, reply('stop'));
    expect(state.settling).toBe(true);
    reduce(state, { type: 'message_update', assistantMessageEvent: { type: 'text_delta' } });
    expect(state.toolsRunning).toBe(0);
    expect(state.settling).toBe(true);
    reduce(state, { type: 'turn_start' });
    expect(state.settling).toBe(false);
  });

  it('turns thinking deltas into a live gist and resets it at the next block', () => {
    const state = newChildState();
    const delta = (text: string) => reduce(state, { type: 'message_update', assistantMessageEvent: { type: 'thinking_delta', delta: text } });
    expect(progress(delta('First, I read the file. Then'))).toEqual([{ thinking: 'First, I read the file.', usage: state.usage }]);
    // The same gist is not reported twice.
    expect(delta(' more')).toEqual([]);
    reduce(state, { type: 'message_update', assistantMessageEvent: { type: 'thinking_start' } });
    expect(state.thought.gist).toBeUndefined();
  });

  it('ends a reply: adds usage, reports narration and the reply, and hands its text to the report', () => {
    const state = newChildState();
    const effects = reduce(state, reply('toolUse', 'Reading the file'));
    expect(state.usage).toMatchObject({ input: 3, output: 2 });
    expect(state.settling).toBe(false);
    expect(state.lastActivity).toBe('model turn');
    const updates = progress(effects);
    expect(updates.map((update) => update.step)).toEqual(['› Reading the file', undefined]);
    expect(updates[1]).toMatchObject({ thinking: '', reply: { role: 'assistant' } });
    expect(effects.at(-1)).toEqual({ kind: 'answer', text: 'Reading the file', stopReason: 'toolUse', errorMessage: undefined });

    const done = reduce(state, reply('stop', 'Done'), NOW + 7);
    expect(state.settling).toBe(true);
    expect(state.lastActivity).toBe('answered; settling');
    expect(state.busyUntil).toBe(NOW + 7 + LIMITS.busyMs + LIMITS.idleMs);
    // A final answer is the report, not a narration step.
    expect(progress(done).map((update) => update.step)).toEqual([undefined]);
  });

  it('reports a message that is not the child\'s own reply as incoming', () => {
    expect(reduce(newChildState(), { type: 'message_end', message: { role: 'user', content: [] } })).toEqual([{ kind: 'incoming', role: 'user' }]);
  });

  it('hands dialogs to the relay and ignores calls a tool makes itself', () => {
    const state = newChildState();
    const dialog = { type: 'extension_ui_request', id: '1', method: 'confirm' };
    expect(reduce(state, dialog)).toEqual([{ kind: 'ui', event: dialog }]);
    expect(reduce(state, { type: 'tool_execution_start', toolName: 'bash', args: {}, parentToolCallId: 'p' })).toEqual([]);
    expect(reduce(state, { type: 'agent_start', parentToolCallId: 'p' })).toEqual([]);
    expect(state).toMatchObject({ toolsRunning: 0, running: false });
  });

  it('answers only the prompt command: a refusal fails the run, a handled prompt closes input', () => {
    const state = newChildState();
    expect(reduce(state, { type: 'response', command: 'prompt', success: false, error: 'busy' })).toEqual([{ kind: 'rejected', message: 'Subagent rejected its task: busy' }]);
    expect(reduce(state, { type: 'response', command: 'prompt', success: false })).toEqual([{ kind: 'rejected', message: 'Subagent rejected its task: no reason given' }]);
    expect(reduce(state, { type: 'response', command: 'prompt', success: true, data: { disposition: 'handled' } })).toEqual([{ kind: 'close' }]);
    expect(reduce(state, { type: 'response', command: 'prompt', success: true })).toEqual([]);
    expect(reduce(state, { type: 'response', command: 'abort', success: false })).toEqual([]);
  });
});

describe('reduceEvent: compaction', () => {
  const end = (fields: Record<string, unknown>) => ({ type: 'compaction_end', reason: 'threshold', aborted: false, willRetry: false, ...fields });
  const usage = { input: 40, output: 10, cacheRead: 0, cacheWrite: 0, totalTokens: 50, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0.004 } };

  it('shows the row compacting while it runs, and not once it ended', () => {
    const state = newChildState();
    expect(progress(reduce(state, { type: 'compaction_start', reason: 'threshold' }))).toEqual([{ compacting: true, usage: state.usage }]);
    expect(state.lastActivity).toBe('compacting its context');
    expect(progress(reduce(state, end({ aborted: true })))).toEqual([{ compacting: false, compactions: { done: 0, aborted: 1, failed: 0 }, usage: state.usage }]);
  });

  it('counts a finished compaction with its sizes, and adds its own usage to the run once', () => {
    const state = newChildState();
    reduce(state, reply('stop'));
    const before = state.usage.cost.total;
    reduce(state, end({ result: { summary: 's', firstKeptEntryId: 'e', tokensBefore: 150_000, estimatedTokensAfter: 32_000, usage, details: {} } }));
    expect(state.compactions).toEqual({ done: 1, aborted: 0, failed: 0, tokensBefore: 150_000, tokensAfter: 32_000 });
    expect(state.usage).toMatchObject({ input: 43, output: 12, totalTokens: 55 });
    expect(state.usage.cost.total).toBeCloseTo(before + 0.004);
    // The next reply adds its own only: the compaction's usage is not carried into later totals twice.
    reduce(state, reply('stop'));
    expect(state.usage).toMatchObject({ input: 46, output: 14 });
  });

  it('records a failed compaction with its message, and a cancelled one without an error', () => {
    const state = newChildState();
    reduce(state, end({ errorMessage: 'provider down' }));
    reduce(state, end({ errorMessage: '' }));
    reduce(state, end({ aborted: true }));
    expect(state.compactions).toEqual({ done: 0, aborted: 1, failed: 2, error: 'compaction failed' });
    expect(state.usage.input).toBe(0);
  });

  it('ignores the compaction of a call a tool made itself', () => {
    const state = newChildState();
    expect(reduce(state, { type: 'compaction_start', parentToolCallId: 'p' })).toEqual([]);
    expect(reduce(state, end({ parentToolCallId: 'p', result: { tokensBefore: 1, estimatedTokensAfter: 1, usage } }))).toEqual([]);
    expect(state.compactions).toEqual({ done: 0, aborted: 0, failed: 0 });
  });
});

describe('reduceEvent: notices', () => {
  const notify = (notifyType: unknown, message: string, id = message) => ({ type: 'extension_ui_request', id, method: 'notify', message, notifyType });

  it('keeps warnings and errors, still hands the event to the relay, and leaves info and audit entries out', () => {
    const state = newChildState();
    const warning = notify('warning', 'summary fell back');
    expect(reduce(state, warning)).toEqual([{ kind: 'ui', event: warning }, { kind: 'progress', update: { notices: { kept: [{ level: 'warning', text: 'summary fell back', count: 1 }], more: 0 }, usage: state.usage } }]);
    expect(reduce(state, notify('info', 'just so you know'))).toHaveLength(1);
    expect(reduce(state, notify('warning', 'octocode-audit/1 {}'))).toHaveLength(1);
    expect(reduce(state, { type: 'extension_ui_request', id: 'x', method: 'setStatus', message: 'm', notifyType: 'error' })).toHaveLength(1);
    expect(reduce(state, notify(undefined, 'no level'))).toHaveLength(1);
    expect(state.notices).toEqual({ kept: [{ level: 'warning', text: 'summary fell back', count: 1 }], more: 0 });
  });

  it('keeps five, counts the rest, and remembers the latest error even past the five', () => {
    const state = newChildState();
    for (let n = 1; n <= 6; n++) reduce(state, notify('warning', `w${n}`));
    reduce(state, notify('error', 'the reason'));
    reduce(state, notify('warning', 'w8'));
    expect(state.notices.kept.map((notice) => notice.text)).toEqual(['w1', 'w2', 'w3', 'w4', 'w5']);
    expect(state.notices).toMatchObject({ more: 3, lastError: 'the reason' });
  });

  it('names an error as the stop reason only when no model turn followed it', () => {
    const state = newChildState();
    reduce(state, notify('error', 'earlier error'));
    reduce(state, reply('toolUse'));
    expect(state.notices.lastError).toBeUndefined();
    // The error stays in the report's notices; it is just not the reason the run stopped.
    expect(state.notices.kept.map((notice) => notice.text)).toEqual(['earlier error']);
    reduce(state, notify('error', 'the stop reason'));
    expect(state.notices.lastError).toBe('the stop reason');
  });

  it('shows hidden characters in a notice, which is kept whole (see notices-whole.test.ts)', () => {
    const state = newChildState();
    reduce(state, notify('error', `bad\u001b[31m ${'x'.repeat(2_000)}`));
    const [first] = state.notices.kept;
    expect(first!.text).toContain('\\u{1b}');
    expect(first!.text).toHaveLength('bad\\u{1b}[31m '.length + 2_000);
  });
});
