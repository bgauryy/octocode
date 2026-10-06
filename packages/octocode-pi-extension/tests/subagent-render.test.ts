import { describe, expect, it } from 'vitest';
import { doneSummary, renderAgentCall, renderAgentMessage, renderAgentResult, type RunDetails } from '../src/subagents/render.js';
import { rendered, theme } from './fake-pi.js';

const ctx = (extra: Record<string, unknown> = {}) => ({ args: {}, lastComponent: undefined, isPartial: false, isError: false, expanded: false, state: {}, ...extra }) as never;
const details = (extra: Partial<RunDetails> = {}): RunDetails => ({ id: 'general-0001', profile: 'general', toolCalls: 2, activity: ['→ bash ls', '→ read a.ts'], input: 1_500, output: 500, startedAt: 0, status: 'done', ...extra });

describe('subagent renderers', () => {
  it('summarizes a finished run with and without a duration', () => {
    expect(doneSummary(details(), 38_000)).toBe('Done (2 tool calls · 2k tokens · 38s)');
    expect(doneSummary(details({ toolCalls: 1 }), undefined)).toBe('Done (1 tool call · 2k tokens)');
  });

  it('draws the call with the profile and tags, never the prompt', () => {
    const call = rendered(renderAgentCall({ task: 'look\nsecond', profile: 'researcher', background: true }, theme, ctx()));
    expect(call).toContain('Agent(researcher)');
    expect(call).not.toContain('look');
    expect(rendered(renderAgentCall({ task: 'x', profile: 'implementer', model: 'openai/gpt-5', isolate: true }, theme, ctx()))).toContain('Agent(implementer · openai/gpt-5 · isolated)');
    expect(rendered(renderAgentCall({ background: true }, theme, ctx()))).toMatch(/Agent\(general\).*background/);
  });

  it('draws a background start, a seconds fallback and a run without details', () => {
    const background = rendered(renderAgentResult({ content: [{ type: 'text', text: 'started' }], details: details({ status: 'background' }) }, { isPartial: false, expanded: false }, theme, ctx()));
    expect(background).toContain('Running in the background as general-0001');
    expect(background).not.toContain('started');
    // Only model-directed text behind it: no expand hint. A later note (an isolation warning) can be expanded to.
    expect(background).not.toContain('to expand');
    const warned = (expanded: boolean) => rendered(renderAgentResult({ content: [{ type: 'text', text: 'Started general-0001 in the background.\n\nNot isolated: no git repo' }], details: details({ status: 'background' }) }, { isPartial: false, expanded }, theme, ctx({ expanded })));
    expect(warned(false)).toContain('to expand');
    expect(warned(true)).toContain('Not isolated: no git repo');
    expect(warned(true)).not.toContain('Started general-0001');
    const fallback = rendered(renderAgentResult({ content: [{ type: 'text', text: 'answer' }], details: details({ seconds: 90 }) }, { isPartial: false, expanded: false }, theme, ctx()));
    expect(fallback).toContain('Done (2 tool calls · 2k tokens · 1m30s)');
    const bare = rendered(renderAgentResult({ content: [{ type: 'text', text: 'Subagent failed\ndetail' }], details: undefined }, { isPartial: false, expanded: false }, theme, ctx({ isError: true })));
    expect(bare).toContain('Subagent failed');
  });

  it('shows the full prompt above the report only when expanded', () => {
    const args = { task: 'Find the bug\n\x1b]0;evil\x07in auth.ts', profile: 'researcher' };
    const run = (expanded: boolean) => rendered(renderAgentResult({ content: [{ type: 'text', text: 'The answer' }], details: details() }, { isPartial: false, expanded }, theme, ctx({ args, expanded })));
    expect(run(false)).not.toContain('Find the bug');
    expect(run(false)).toContain('The answer');
    expect(run(false)).toContain('to expand');
    const open = run(true);
    expect(open).toMatch(/Prompt\n\s+Find the bug\n\s+in auth\.ts\n\s*\n\s+Report\n\s+The answer/);
    expect(open).not.toContain('\x1b]0;');
    const live = (expanded: boolean) => rendered(renderAgentResult({ content: [], details: details({ status: 'running' }) }, { isPartial: true, expanded }, theme, ctx({ args, expanded, isPartial: true, executionStarted: true })));
    expect(live(false)).toContain('general-0001 · 2 tool calls');
    expect(live(true)).toMatch(/Prompt[\s\S]*Progress[\s\S]*→ read a\.ts/);
  });

  it('shows a background report with its full prompt instead of the task line', () => {
    const message = (expanded: boolean) => rendered(renderAgentMessage({ role: 'custom', customType: 'x', content: 'Background subagent general-0001 finished (3s · 1 tool call):\nTask: Fix login\nAll fixed.', details: { id: 'general-0001', status: 'done', summary: 'Done (1 tool call)', durationMs: 3_000, task: 'Fix login\nin src/auth.ts' }, display: true, timestamp: 0 } as never, { expanded, outputPad: 1 }, theme as never) as never);
    expect(message(false)).toContain('Agent(general-0001)');
    expect(message(false)).not.toContain('Fix login');
    expect(message(false)).toContain('All fixed.');
    expect(message(true)).toMatch(/Prompt\n\s+Fix login\n\s+in src\/auth\.ts\n\s*\n\s+Report\n\s+All fixed\./);
    expect(message(true)).not.toContain('Task: ');
  });

  it('draws a background report message without details as a plain report', () => {
    const box = renderAgentMessage({ role: 'custom', customType: 'x', content: 'Report head\nbody', display: true, timestamp: 0 } as never, { expanded: true, outputPad: 1 }, theme as never);
    const text = rendered(box as never);
    expect(text).toContain('Agent(background)');
    expect(text).toContain('Report head');
    expect(text).toContain('body');
    const failed = rendered(renderAgentMessage({ role: 'custom', customType: 'x', content: 'x', display: true, timestamp: 0, details: { id: 'a-1', status: 'failed', summary: 'Failed: boom', durationMs: 1_200 } } as never, { expanded: false, outputPad: 2 }, theme as never) as never);
    expect(failed).toMatch(/Agent\(a-1\).*1\.2s/);
    expect(failed).toContain('Failed: boom');
    expect(failed).not.toContain('Error: Failed');
  });

  it('draws any stop as an interruption (◼ warning), never as a failure', () => {
    const colored = { ...(theme as object), fg: (color: string, text: string) => `<${color}>${text}` };
    const stopped = (requested: boolean) => rendered(renderAgentMessage({ role: 'custom', customType: 'x', content: 'x', display: true, timestamp: 0, details: { id: 'a-1', status: 'stopped', summary: 'Stopped', requested } } as never, { expanded: false, outputPad: 2 }, colored as never) as never);
    for (const requested of [true, false]) {
      expect(stopped(requested)).toContain('<warning>◼');
      expect(stopped(requested)).not.toContain('<error>');
    }
  });
});
