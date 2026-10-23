import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { registerCompaction } from '../src/compaction/register.js';
import { bindsThinkingToPrefix, stripStaleThinking } from '../src/compaction/thinking.js';
import type { FileGuard } from '../src/files/guard.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

const T = 1_700_000_000_000;
const thinking = (text: string) => ({ type: 'thinking', thinking: text, thinkingSignature: `sig-${text}` });
const redacted = { type: 'thinking', thinking: '[Reasoning redacted]', thinkingSignature: 'opaque', redacted: true };
const call = (id: string) => ({ type: 'toolCall', id, name: 'read', arguments: { path: 'a.ts' } });
const assistant = (at: number, ...content: unknown[]) => ({ role: 'assistant', content, timestamp: at });
const result = (id: string, at: number) => ({ role: 'toolResult', toolCallId: id, toolName: 'read', content: [{ type: 'text', text: 'body' }], isError: false, timestamp: at });
const summary = (at: number) => ({ role: 'compactionSummary', summary: '## Goal\nfix it', tokensBefore: 200_000, timestamp: at });

// A compaction at T + 100 kept two turns produced before it; one turn followed it.
const kept = [assistant(T + 10, thinking('old'), call('c1')), result('c1', T + 11), assistant(T + 20, redacted, { type: 'text', text: 'next' }, call('c2')), result('c2', T + 21)];
const fresh = assistant(T + 200, thinking('new'), call('c3'));
const compacted = [summary(T + 100), ...kept, fresh, result('c3', T + 201)];
const thinkingOf = (messages: any[]) => messages.flatMap((message) => (message.role === 'assistant' ? message.content.filter((part: any) => part.type === 'thinking').map((part: any) => part.thinking) : []));

describe('stale thinking after a compaction', () => {
  it('is the thinking of turns older than a summary before them: removed, the rest of each turn unchanged', () => {
    const out = stripStaleThinking(compacted);
    expect(thinkingOf(out)).toEqual(['new']);
    expect(out[1]).toEqual(assistant(T + 10, call('c1')));
    expect(out[3]).toEqual(assistant(T + 20, { type: 'text', text: 'next' }, call('c2')));
    // Everything else is the same object: the request changes only where thinking went.
    for (const index of [0, 2, 4, 5, 6]) expect(out[index]).toBe(compacted[index]);
    // Pure: the session's own messages are not touched.
    expect(thinkingOf(compacted)).toEqual(['old', '[Reasoning redacted]', 'new']);
  });

  it('leaves the list as it is with no summary, or after a branch summary, which only precedes newer turns', () => {
    const plain = [{ role: 'user', content: 'go', timestamp: T }, ...kept];
    expect(stripStaleThinking(plain)).toBe(plain);
    const branched = [{ role: 'user', content: 'go', timestamp: T }, { role: 'branchSummary', summary: 'tried x', fromId: 'e1', timestamp: T + 5 }, ...kept];
    expect(stripStaleThinking(branched)).toBe(branched);
  });

  it('matters only for a model that binds thinking to its prefix (pi-ai sends drop_block for it)', () => {
    expect(bindsThinkingToPrefix({ compat: { supportsMidConvoEffort: true } })).toBe(true);
    expect(bindsThinkingToPrefix({ compat: {} })).toBe(false);
    expect(bindsThinkingToPrefix(undefined)).toBe(false);
  });
});

describe('every request after a compaction', () => {
  let saved: string | undefined;
  beforeEach(() => {
    saved = process.env['OCTOCODE_HOME'];
    process.env['OCTOCODE_HOME'] = tmp();
  });
  afterEach(() => {
    if (saved === undefined) delete process.env['OCTOCODE_HOME'];
    else process.env['OCTOCODE_HOME'] = saved;
  });

  async function context(model: unknown) {
    const fake = fakePi();
    registerCompaction(fake.pi, { restore: () => undefined, forget: () => undefined } as unknown as FileGuard);
    const ctx = fakeCtx({ cwd: '/repo', model });
    Object.assign(ctx, { sessionManager: { getSessionId: () => 's1', getBranch: () => [], getEntries: () => [] } });
    const out = (await fake.fire('context', { type: 'context', messages: structuredClone(compacted) }, ctx)) as { messages?: unknown[] } | undefined;
    return out?.messages;
  }

  it('sends no thinking from before it to a model that binds thinking to its prefix, and changes nothing for another', async () => {
    const binding = { provider: 'p', id: 'm', api: 'anthropic-messages', compat: { supportsMidConvoEffort: true } };
    expect(thinkingOf((await context(binding))!)).toEqual(['new']);
    expect(await context({ ...binding, compat: {} })).toBeUndefined();
  });
});
