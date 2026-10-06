import { afterEach, describe, expect, it, vi } from 'vitest';
import { Activity, registerActivity, shortToolName } from '../src/ui/activity.js';
import { withDialog } from '../src/shared/locks.js';
import { fakeCtx, fakePi } from './fake-pi.js';

afterEach(() => vi.useRealTimers());

describe('Activity', () => {
  it('describes model phases, parallel tools, nested calls and user prompts', () => {
    const activity = new Activity();
    expect(activity.describe(0)).toBeUndefined();
    activity.start(0);
    expect(activity.describe(0)).toBeUndefined();
    activity.stream('thinking_delta', 0);
    expect(activity.describe(1_000)).toBe('Thinking');
    expect(activity.describe(5_000)).toBe('Thinking · 5s');
    expect(activity.describe()).toBe('Thinking');
    activity.stream('text_delta', 5_000);
    expect(activity.describe(9_000)).toBe('Writing');
    activity.stream('toolcall_start', 9_000);
    expect(activity.describe(9_000)).toBe('Preparing tool calls');
    activity.stream('done', 9_000);
    activity.messageDone(9_000);
    expect(activity.describe(9_000)).toBeUndefined();

    activity.toolStart('1', 'bash', { command: 'yarn test\nmore' }, 10_000);
    expect(activity.describe(10_500)).toBe('Running bash yarn test');
    activity.toolStart('2', 'mcp__octocode__localSearch', { path: '/repo' }, 11_000);
    activity.toolStart('3', 'mcp__octocode__localSearch', {}, 11_000);
    expect(activity.describe(13_000)).toBe('Running 3 tools: bash · localSearch ×2 · 3s');
    expect(activity.ticking()).toBe(true);
    activity.promptStart('Pick one', 13_000);
    expect(activity.describe(14_000)).toBe('Waiting for you: Pick one');
    activity.promptEnd();
    activity.toolEnd('1', 14_000);
    activity.toolEnd('2', 14_000);
    activity.toolEnd('unknown', 14_000);
    expect(activity.describe(14_000)).toBe('Running localSearch · 3s');
    activity.toolEnd('3', 15_000);
    expect(activity.describe(15_000)).toBeUndefined();
    expect(activity.ticking()).toBe(false);
    activity.promptStart(undefined, 15_000);
    expect(activity.describe(15_000)).toBe('Waiting for you');
    activity.end();
    expect(activity.describe(15_000)).toBeUndefined();
    // A message ending after the run ended does not revive it.
    activity.messageDone(16_000);
    expect(activity.phase).toBe('idle');
  });

  it('shortens MCP tool names and sanitizes hints', () => {
    expect(shortToolName('mcp__octocode__localSearch')).toBe('localSearch');
    expect(shortToolName('mcp__my_server__get_file')).toBe('get_file');
    expect(shortToolName('bash')).toBe('bash');
    const activity = new Activity();
    activity.start(0);
    activity.toolStart('1', 'bash', { command: 'echo \u001b[31mred' }, 0);
    expect(activity.describe(0)).not.toContain('\u001b');
  });
});

describe('registerActivity', () => {
  it('drives the working line and the team activity from Pi events', async () => {
    vi.useFakeTimers();
    const fake = fakePi();
    const lines: Array<string | undefined> = [];
    registerActivity(fake.pi, { workingLine: true, onChange: (line) => lines.push(line) });
    const ctx = fakeCtx({ cwd: '/x' });
    const ui = ctx.ui as typeof ctx.ui & { working?: string };
    await fake.emit('session_start', {}, ctx);
    await fake.emit('agent_start', {}, ctx);
    await fake.emit('message_update', { message: { role: 'assistant' }, assistantMessageEvent: { type: 'thinking_delta' } }, ctx);
    await fake.emit('message_update', { message: { role: 'user' }, assistantMessageEvent: { type: 'text_delta' } }, ctx);
    expect(ui.working).toBe('Thinking');
    await fake.emit('message_end', { message: { role: 'assistant' } }, ctx);
    await fake.emit('message_end', { message: { role: 'user' } }, ctx);
    await fake.emit('tool_execution_start', { toolCallId: 'a', toolName: 'bash', args: { command: 'sleep 9' } }, ctx);
    // A nested call a tool makes is part of that call.
    await fake.emit('tool_execution_start', { toolCallId: 'a/1', parentToolCallId: 'a', toolName: 'web', args: {} }, ctx);
    expect(ui.working).toBe('Running bash sleep 9');
    vi.advanceTimersByTime(3_000);
    expect(ui.working).toBe('Running bash sleep 9 · 3s');
    await fake.emit('ui_prompt_start', { title: 'Continue?' }, ctx);
    expect(ui.working).toBe('Waiting for you: Continue?');
    await fake.emit('ui_prompt_end', {}, ctx);
    await fake.emit('tool_execution_end', { toolCallId: 'a/1', parentToolCallId: 'a' }, ctx);
    expect(ui.working).toBe('Running bash sleep 9 · 3s');
    await fake.emit('tool_execution_end', { toolCallId: 'a' }, ctx);
    expect(ui.working).toBeUndefined();
    await fake.emit('agent_settled', {}, ctx);
    expect(ui.working).toBeUndefined();
    // The team hears state changes only, never the ticking elapsed time.
    expect(lines).toEqual(['Thinking', undefined, 'Running bash sleep 9', 'Waiting for you: Continue?', 'Running bash sleep 9', undefined]);
    await fake.emit('session_shutdown', {}, ctx);
  });

  it('skips the working line for subagents and headless runs', async () => {
    const fake = fakePi();
    registerActivity(fake.pi, { workingLine: false });
    const ctx = fakeCtx({ cwd: '/x' });
    await fake.emit('agent_start', {}, ctx);
    await fake.emit('tool_execution_start', { toolCallId: 'a', toolName: 'bash', args: {} }, ctx);
    expect((ctx.ui as { working?: string }).working).toBeUndefined();
    const headless = fakePi();
    registerActivity(headless.pi, { workingLine: true });
    const quiet = fakeCtx({ cwd: '/x', hasUI: false });
    await headless.emit('agent_start', {}, quiet);
    await headless.emit('tool_execution_start', { toolCallId: 'a', toolName: 'bash', args: {} }, quiet);
    expect((quiet.ui as { working?: string }).working).toBeUndefined();
  });

  it('shows a call\'s pre-run checks and dialogs queued behind the open one', async () => {
    vi.useFakeTimers();
    const fake = fakePi();
    const control = registerActivity(fake.pi, { workingLine: true });
    const ctx = fakeCtx({ cwd: '/x' });
    const ui = ctx.ui as typeof ctx.ui & { working?: string };
    await fake.emit('session_start', {}, ctx);
    await fake.emit('agent_start', {}, ctx);
    await fake.emit('tool_execution_start', { toolCallId: 'a', toolName: 'bash', args: { command: 'ls' } }, ctx);
    await fake.emit('tool_execution_start', { toolCallId: 'b', toolName: 'web', args: {} }, ctx);
    control.checking('a', true);
    expect(ui.working).toBe('Checking bash ls');
    vi.advanceTimersByTime(2_000);
    expect(ui.working).toBe('Checking bash ls · 2s');
    control.checking('a', false);
    control.checking('missing', true);
    expect(ui.working).toBe('Running 2 tools: bash · web · 2s');
    let close!: () => void;
    const open = withDialog(() => new Promise<void>((resolve) => (close = resolve)));
    await fake.emit('ui_prompt_start', { title: 'Pick' }, ctx);
    const queued = withDialog(async () => undefined);
    await Promise.resolve();
    expect(ui.working).toMatch(/^Waiting for you: Pick \(\+1 queued\)/);
    close();
    await Promise.all([open, queued]);
    await fake.emit('ui_prompt_end', {}, ctx);
    await fake.emit('session_shutdown', {}, ctx);
    vi.useRealTimers();
  });
});
