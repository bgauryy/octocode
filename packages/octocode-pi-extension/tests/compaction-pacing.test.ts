import { reserveTokensFor } from '../src/shared/tokens.js';
import { describe, expect, it, vi } from 'vitest';
import { CHARS_PER_TOKEN } from '../src/shared/tokens.js';
import { THRASH_STREAK, THRASH_TURNS, ThrashBreaker, contextTokens, needsDownshiftCompaction, toolDefinitionChars } from '../src/compaction/pacing.js';
import { COMPACTION_NOTE_TYPE } from '../src/compaction/record.js';
import { registerCompaction } from '../src/compaction/register.js';

describe('thrash breaker', () => {
  const refill = (breaker: ThrashBreaker, turns: number) => {
    for (let i = 0; i < turns; i++) breaker.turnEnded();
  };

  it(`trips after ${THRASH_STREAK} compactions in a row that each came within ${THRASH_TURNS} turns of the previous one`, () => {
    const breaker = new ThrashBreaker();
    breaker.compacted(false);
    for (let i = 0; i < THRASH_STREAK; i++) {
      expect(breaker.tripped()).toBe(false);
      refill(breaker, THRASH_TURNS);
      breaker.compacted(false);
    }
    refill(breaker, 1);
    expect(breaker.tripped()).toBe(true);
    // Room to breathe: more turns than the window is not a refill.
    refill(breaker, THRASH_TURNS);
    expect(breaker.tripped()).toBe(false);
  });

  it('starts over after a slow refill, a manual compaction or a new session', () => {
    const breaker = new ThrashBreaker();
    breaker.compacted(false);
    for (let i = 0; i < THRASH_STREAK; i++) {
      refill(breaker, 1);
      breaker.compacted(false);
    }
    breaker.compacted(true);
    refill(breaker, 1);
    expect(breaker.tripped()).toBe(false);
    for (let i = 0; i < THRASH_STREAK; i++) {
      refill(breaker, 1);
      breaker.compacted(false);
    }
    breaker.reset();
    expect(breaker.tripped()).toBe(false);
    refill(breaker, THRASH_TURNS + 1);
    breaker.compacted(false);
    refill(breaker, 1);
    expect(breaker.tripped()).toBe(false);
  });
});

describe('downshift compaction', () => {
  it('compacts only when the context does not fit a smaller window less the reply room', () => {
    const big = { contextWindow: 1_000_000 };
    const small = { contextWindow: 200_000 };
    expect(needsDownshiftCompaction(big, small, 190_000, 16_384)).toBe(true);
    expect(needsDownshiftCompaction(big, small, 150_000, 16_384)).toBe(false);
    expect(needsDownshiftCompaction(small, big, 190_000, 16_384)).toBe(false);
    expect(needsDownshiftCompaction(undefined, small, 190_000, 16_384)).toBe(false);
    expect(needsDownshiftCompaction(big, small, null, 16_384)).toBe(false);
  });

  it('estimates the context when Pi has no count (after a compaction, until the next reply)', () => {
    const projection = { entries: [{ messages: [{ role: 'user', content: 'x'.repeat(400_000), timestamp: 0 }] }] };
    const ctx = (tokens: number | null) => ({ getContextUsage: () => ({ tokens, contextWindow: 1_000_000, percent: null }), getSystemPrompt: () => 'y'.repeat(4_000), sessionManager: { buildSessionProjection: () => projection } });
    expect(contextTokens(ctx(42) as never)).toBe(42);
    expect(contextTokens(ctx(null) as never)).toBe(Math.ceil(4_000 / CHARS_PER_TOKEN) + 100_000);
    expect(contextTokens(ctx(null) as never, 3_000)).toBe(Math.ceil(7_000 / CHARS_PER_TOKEN) + 100_000);
    expect(contextTokens(ctx(42) as never, 3_000)).toBe(42);
    expect(needsDownshiftCompaction({ contextWindow: 1_000_000 }, { contextWindow: 100_000 }, contextTokens(ctx(null) as never), 16_384)).toBe(true);
  });
});

describe('tool definitions in the estimate', () => {
  const tool = (name: string) => ({ name, description: `does ${name}`, parameters: { type: 'object', properties: { q: { type: 'string' } } }, promptGuidelines: ['be careful'], exposure: 'direct', sourceInfo: {} });
  it('serializes only the active tools', () => {
    const pi = { getActiveTools: () => ['a'], getAllTools: () => [tool('a'), tool('b')] };
    const { name, description, parameters, promptGuidelines } = tool('a');
    expect(toolDefinitionChars(pi as never)).toBe(JSON.stringify({ name, description, parameters, promptGuidelines }).length);
    expect(toolDefinitionChars({ getActiveTools: () => [], getAllTools: () => [tool('a')] } as never)).toBe(0);
  });
  it('counts as zero when Pi cannot list them', () => {
    expect(toolDefinitionChars({ getActiveTools: () => { throw new Error('gone'); }, getAllTools: () => [] } as never)).toBe(0);
  });
});

describe('pacing wired into the session', () => {
  function session(tokens: number | null, tools: unknown[] = []) {
    const handlers = new Map<string, (event: unknown, ctx: unknown) => Promise<unknown>>();
    const notes: Array<{ customType: string; data: unknown }> = [];
    const pi = { on: (name: string, handler: never) => void handlers.set(name, handler), appendEntry: (customType: string, data: unknown) => notes.push({ customType, data }), getSettings: () => ({ compaction: { reserveTokens: 10_000 } }), getThinkingLevel: () => 'off', getActiveTools: () => tools.map((t) => (t as { name: string }).name), getAllTools: () => tools, sendMessage: vi.fn() };
    registerCompaction(pi as never, { restore: vi.fn(), forget: vi.fn(), record: vi.fn() } as never);
    const ctx = {
      cwd: '/w', hasUI: true, ui: { notify: vi.fn() }, abort: vi.fn(), compact: vi.fn(), model: undefined,
      getContextUsage: () => ({ tokens, contextWindow: 1, percent: 1 }),
      getSystemPrompt: () => '',
      sessionManager: { getSessionId: () => 's', getBranch: () => [], buildSessionProjection: () => ({ entries: [] }), getSessionFile: () => undefined },
    };
    const fire = (name: string, event: unknown = {}) => handlers.get(name)!(event, ctx);
    return { ctx, fire, notes };
  }
  const before = (reason: string) => ({ reason, preparation: { messagesToSummarize: [], turnPrefixMessages: [], settings: { enabled: true, reserveTokens: 16_384, keepRecentTokens: 20_000 }, fileOps: { read: new Set(), written: new Set(), edited: new Set() } }, branchEntries: [], signal: new AbortController().signal });
  const turnEnd = { message: { role: 'assistant' }, messageEntryId: 'm', entries: [], context: { contextEntries: [] } };

  it('cancels an automatic compaction and stops the run once the context keeps refilling; a manual one still runs', async () => {
    const { ctx, fire, notes } = session(1);
    await fire('session_compact', { reason: 'threshold' });
    for (let i = 0; i < THRASH_STREAK; i++) {
      await fire('turn_end', turnEnd);
      await fire('session_compact', { reason: 'threshold' });
    }
    await fire('turn_end', turnEnd);
    expect(await fire('session_before_compact', before('threshold'))).toEqual({ cancel: true });
    expect(ctx.abort).toHaveBeenCalled();
    expect(ctx.ui.notify).toHaveBeenCalledWith(expect.stringMatching(/Automatic compaction stopped/), 'error');
    // The refusal is also durable: one session entry, with the same reason the user saw.
    expect(notes).toEqual([{ customType: COMPACTION_NOTE_TYPE, data: { kind: 'thrash-stop', reason: expect.stringMatching(/Automatic compaction stopped/) } }]);
    expect(await fire('session_before_compact', before('manual'))).toBeUndefined();
  });

  it('compacts before switching to a model whose window the context does not fit', async () => {
    const { ctx, fire, notes } = session(150_000);
    await fire('model_select', { model: { id: 'small', contextWindow: 128_000 }, previousModel: { id: 'big', contextWindow: 1_000_000 }, source: 'set' });
    expect(ctx.compact).toHaveBeenCalledTimes(1);
    expect(notes).toEqual([{ customType: COMPACTION_NOTE_TYPE, data: { kind: 'downshift', reason: "The context does not fit small's window; compacting first on big." } }]);
    await fire('model_select', { model: { id: 'big', contextWindow: 1_000_000 }, previousModel: { id: 'small', contextWindow: 128_000 }, source: 'set' });
    expect(ctx.compact).toHaveBeenCalledTimes(1);
  });

  it('compacts when the active tools push an uncounted context past the new window', async () => {
    const bulky = { name: 'bulky', description: 'x'.repeat(300_000), parameters: {}, promptGuidelines: [] };
    const select = { model: { id: 'small', contextWindow: 100_000 }, previousModel: { id: 'big', contextWindow: 1_000_000 }, source: 'set' };
    const without = session(null);
    await without.fire('model_select', select);
    expect(without.ctx.compact).not.toHaveBeenCalled();
    const withTools = session(null, [bulky]);
    await withTools.fire('model_select', select);
    expect(withTools.ctx.compact).toHaveBeenCalledTimes(1);
  });
});

describe('per-model compaction reserve', () => {
  it("reads the model's override, else the global reserve, else Pi's default", () => {
    const model = { provider: 'p', id: 'm' };
    expect(reserveTokensFor({ compaction: { reserveTokens: 40_000, modelOverrides: { 'p/m': { reserveTokens: 8_000 } } } }, model)).toBe(8_000);
    expect(reserveTokensFor({ compaction: { reserveTokens: 40_000, modelOverrides: { 'p/other': { reserveTokens: 8_000 } } } }, model)).toBe(40_000);
    expect(reserveTokensFor({}, model)).toBe(16_384);
  });
});
