import { describe, expect, it } from 'vitest';
import { formatCost, formatTokens, formatTokenUsage } from '../src/shared/format.js';
import { addReply, emptyTotals, entryMessage, spendOf, unpricedModel } from '../src/shared/util.js';

const replyUsage = (total: number) => ({ input: 10, output: 5, cacheRead: 90, cacheWrite: 0, cost: { total } });
const rates = (rate: number) => ({ cost: { input: rate, output: rate, cacheRead: rate, cacheWrite: rate } });
const find = (provider: string) => (provider === 'free' ? rates(0) : provider === 'paid' ? rates(3) : undefined);

describe('usage display', () => {
  it('formats token counts up to millions', () => {
    expect([950, 12_400, 250_000, 1_234_567, 143_861_000].map(formatTokens)).toEqual(['950', '12.4k', '250k', '1.2M', '144M']);
  });

  it('counts every prompt token in ↑ and shows the share read from cache', () => {
    expect(formatTokenUsage({ input: 1_200_000, cacheRead: 1_128_000, output: 35_000 })).toBe('↑1.2M (94% cached) ↓35k');
    expect(formatTokenUsage({ input: 1_500, cacheRead: 0, output: 250 })).toBe('↑1.5k ↓250');
  });

  it('shows money as an estimate and never hides an unpriced part', () => {
    expect(formatCost({ cost: 2.2849, unpriced: false })).toBe('≈$2.28');
    expect(formatCost({ cost: 0.004, unpriced: false })).toBe('≈$<0.01');
    expect(formatCost({ cost: 2.28, unpriced: true })).toBe('≈$2.28 + n/a');
    expect(formatCost({ cost: 0, unpriced: true })).toBe('cost n/a');
    expect(formatCost({ cost: 0, unpriced: false })).toBe('');
  });

  it('marks a reply unpriced when its model has no rates and it spent tokens', () => {
    expect(unpricedModel({ provider: 'free', model: 'llama', usage: replyUsage(0) }, find)).toBe('free/llama');
    expect(unpricedModel({ provider: 'paid', model: 'opus', usage: replyUsage(0.2) }, find)).toBeUndefined();
    // A model the registry does not know is judged by the reply's own cost.
    expect(unpricedModel({ provider: 'gone', model: 'x', usage: replyUsage(0) }, find)).toBe('gone/x');
    expect(unpricedModel({ provider: 'gone', model: 'x', usage: replyUsage(0.1) }, find)).toBeUndefined();
    // Nothing spent, nothing missing.
    expect(unpricedModel({ provider: 'free', model: 'llama', usage: { input: 0, output: 0, cost: { total: 0 } } }, find)).toBeUndefined();
    expect(unpricedModel({ role: 'toolResult', usage: replyUsage(0) })).toBe('unknown model');
  });

  it('adds a reply into display totals: the whole prompt, its cached part, cost and whether any part was unpriced', () => {
    const totals = emptyTotals();
    addReply(totals, { input: 10, output: 5, cacheRead: 80, cacheWrite: 10, cost: { total: 0.5 } }, false);
    addReply(totals, { input: 100, output: 1, cost: { total: 0 } }, true);
    addReply(totals, 'not usage', false);
    expect(totals).toEqual({ input: 200, cacheRead: 80, output: 6, cost: 0.5, unpriced: true });
  });
});

describe('spendOf', () => {
  it('counts replies, tool results with usage and custom messages that report usage, nothing else', () => {
    const usage = { input: 1, output: 2, cost: { total: 0.1 } };
    expect(spendOf({ role: 'assistant', usage })?.usage).toBe(usage);
    expect(spendOf({ role: 'toolResult', usage })?.usage).toBe(usage);
    expect(spendOf({ role: 'toolResult' })).toBeUndefined();
    expect(spendOf({ role: 'user', usage })).toBeUndefined();
    // A background report: the run's usage and its unpriced verdict, judged like an `agent` result (no model named).
    const report = spendOf({ role: 'custom', customType: 'octocode-agent-result', details: { usage, unpriced: true } });
    expect(report).toEqual({ role: 'custom', usage, details: { unpriced: true } });
    expect(unpricedModel(report)).toBe('a subagent model');
    expect(spendOf({ role: 'custom', details: { id: 'x' } })).toBeUndefined();
    expect(spendOf(entryMessage({ type: 'custom_message', customType: 'r', details: { usage } }))?.usage).toBe(usage);
    expect(entryMessage({ type: 'label' })).toBeUndefined();
  });
});
