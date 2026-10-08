import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { KEEP_RECENT_RESULTS, TRIM_PRESSURE_PERCENT, planContextTrims, promptCacheTtlMs, registerCompaction, trimsDue, type TrimMemo } from '../src/compaction/register.js';
import type { FileGuard } from '../src/files/tool.js';

type Handler = (event: unknown, ctx: unknown) => Promise<unknown>;

const call = (index: number, size: number) => ({
  id: `a${index}`,
  message: { role: 'assistant', content: [{ type: 'toolCall', id: `c${index}`, name: 'file', arguments: { queries: [{ type: 'write', path: `f${index}.ts`, content: 'w'.repeat(size) }] } }] },
});
const result = (index: number) => ({ id: `r${index}`, message: { role: 'toolResult', toolName: 'file', toolCallId: `c${index}`, content: [{ type: 'text', text: 'ok' }] } });

/** Pi's turn_end shape: each entry is one session message. */
const turnEnd = (entries: Array<{ id: string; message: unknown }>) => ({
  entries: [],
  context: { contextEntries: entries.map((entry) => ({ sourceEntry: { type: 'message', id: entry.id }, messages: [entry.message] })) },
});

function register(model?: { promptCache?: { short?: number; long?: number } }) {
  const handlers = new Map<string, Handler>();
  const pi = { on: (name: string, handler: Handler) => void handlers.set(name, handler) } as unknown as ExtensionAPI;
  const reset = vi.fn();
  const forget = vi.fn();
  registerCompaction(pi, { reset, forget } as unknown as FileGuard);
  let session = 's1';
  // Context pressure by default, so turn_end plans trims; tests lower it to see the cache-aware deferral.
  const usage = { percent: 80 as number | null };
  const ctx = { model, cwd: '/repo', sessionManager: { getSessionId: () => session }, getContextUsage: () => ({ tokens: 1, contextWindow: 100, percent: usage.percent }) };
  return {
    usage,
    reset,
    forget,
    fire: (name: string, event: unknown = {}) => handlers.get(name)!(event, ctx),
    switchTo: (id: string) => void (session = id),
  };
}

/** How often JSON.stringify ran on the arguments of the given calls. */
function countStringify(entries: Array<{ message: unknown }>) {
  const tracked = new Set(entries.flatMap((entry) => ((entry.message as { content: Array<{ arguments?: unknown }> }).content ?? []).map((part) => part.arguments)).filter(Boolean));
  const original = JSON.stringify;
  let count = 0;
  const spy = vi.spyOn(JSON, 'stringify').mockImplementation((value: unknown, replacer?: (string | number)[] | null, space?: string | number) => {
    if (tracked.has(value)) count++;
    return original(value, replacer, space);
  });
  return { count: () => count, restore: () => spy.mockRestore() };
}

afterEach(() => vi.restoreAllMocks());

describe('trim plan memo', () => {
  it('measures each settled call once across turn_end events, and forgets on session switch, compaction and tree moves', async () => {
    const { fire, switchTo, reset } = register();
    // A small settled file call: judged too small once, then never re-measured.
    const history = [call(0, 100), result(0), call(1, 200), result(1)];
    const counter = countStringify(history);
    expect(await fire('turn_end', turnEnd(history))).toBeUndefined();
    const first = counter.count();
    expect(first).toBe(2);
    const next = [...history, call(2, 50), result(2)];
    expect(await fire('turn_end', turnEnd(next))).toBeUndefined();
    expect(counter.count()).toBe(first);

    switchTo('s2');
    await fire('turn_end', turnEnd(history));
    expect(counter.count()).toBe(first * 2);
    await fire('session_compact');
    expect(reset).toHaveBeenCalledOnce();
    await fire('turn_end', turnEnd(history));
    expect(counter.count()).toBe(first * 3);
    await fire('session_tree');
    await fire('turn_end', turnEnd(history));
    expect(counter.count()).toBe(first * 4);
    await fire('session_start');
    await fire('turn_end', turnEnd(history));
    expect(counter.count()).toBe(first * 5);
    counter.restore();
  });

  it('defers trims while the prompt cache is warm and the context has room', async () => {
    const now = Date.now();
    const ttl = 300_000;
    const turn = (at: number) => ({ message: { role: 'assistant', timestamp: at } });
    // Room in context, cache just written: wait.
    expect(trimsDue({ written: now, ttlMs: ttl }, 10, now)).toBe(false);
    expect(trimsDue({ written: now, ttlMs: ttl }, null, now)).toBe(false);
    // The turn's tools ran past the cache lifetime with no refresh: the next request misses anyway.
    expect(trimsDue({ written: now - ttl, ttlMs: ttl }, 10, now)).toBe(true);
    // The context is filling up: trim even on a warm cache.
    expect(trimsDue({ written: now, ttlMs: ttl }, TRIM_PRESSURE_PERCENT, now)).toBe(true);
    // No prompt cache for the model (or none known): nothing to keep warm.
    expect(trimsDue({ written: now, ttlMs: undefined }, 10, now)).toBe(true);

    // The lifetime is the model's tier for the retention in use.
    const model = { promptCache: { short: 300, long: 3600 } };
    expect(promptCacheTtlMs(model, {})).toBe(300_000);
    expect(promptCacheTtlMs(model, { PI_CACHE_RETENTION: 'long' })).toBe(3_600_000);
    expect(promptCacheTtlMs({}, {})).toBeUndefined();

    const big = Array.from({ length: KEEP_RECENT_RESULTS + 10 }, (_, index) => [call(index, 70_000), result(index)]).flat();
    const session = () => {
      const s = register({ promptCache: { short: 300 } });
      s.usage.percent = 10;
      return s;
    };
    const warm = session();
    expect(await warm.fire('turn_end', { ...turnEnd(big), ...turn(Date.now()) })).toBeUndefined();
    // Pending, not lost: the same history trims once a turn's tools ran past the lifetime...
    const cold = session();
    expect(((await cold.fire('turn_end', { ...turnEnd(big), ...turn(Date.now() - ttl - 1) })) as { entries: unknown[] }).entries.length).toBeGreaterThan(0);
    // ...unless Pi kept the cache warm meanwhile.
    const kept = session();
    await kept.fire('cache_warming_decision', { type: 'cache_warming_decision', action: 'warm' });
    expect(await kept.fire('turn_end', { ...turnEnd(big), ...turn(Date.now() - ttl - 1) })).toBeUndefined();
    const stopped = session();
    await stopped.fire('cache_warming_decision', { type: 'cache_warming_decision', action: 'stop' });
    expect(((await stopped.fire('turn_end', { ...turnEnd(big), ...turn(Date.now() - ttl - 1) })) as { entries: unknown[] }).entries.length).toBeGreaterThan(0);
  });

  it('plans the same edits with a memo, keeps pending shrinks until a batch is worth it, and never re-proposes one', () => {
    // One large file call saves too little alone (below every threshold), so nothing is proposed yet.
    const memo: TrimMemo = new Map();
    const small = [call(0, 3_000), result(0)];
    expect(planContextTrims(small, memo)).toEqual([]);
    expect(memo.get('c0')).toMatchObject({ saved: expect.any(Number) });

    const big = [...small, ...Array.from({ length: KEEP_RECENT_RESULTS }, (_, index) => [call(10 + index, 70_000), result(10 + index)]).flat()];
    const counter = countStringify(small);
    const edits = planContextTrims(big, memo);
    // The pending verdict for c0 was reused, not recomputed.
    expect(counter.count()).toBe(0);
    counter.restore();
    expect(edits).toEqual(planContextTrims(big));
    expect(edits.map((edit) => edit.targetId)).toContain('a0');
    // Once proposed, the same (still unapplied) history proposes nothing new for these calls.
    expect(planContextTrims(big, memo)).toEqual([]);
  });
});

describe('trimmed results', () => {
  const big = (index: number, text: string, toolName = 'bash') => ({ id: `r${index}`, message: { role: 'toolResult', toolName, toolCallId: `c${index}`, content: [{ type: 'text', text }] } });
  const readCall = (index: number, name: string, args: Record<string, unknown>) => ({ id: `a${index}`, message: { role: 'assistant', content: [{ type: 'toolCall', id: `c${index}`, name, arguments: args }] } });
  const filler = (from: number) => Array.from({ length: KEEP_RECENT_RESULTS }, (_, index) => big(from + index, 'x'.repeat(3_500)));
  const textOf = (edit: { replacement: unknown }) => (edit.replacement as { content: Array<{ text: string }> }).content[0]!.text;

  it('keeps the head, the tail and any spill pointer in between, and names the saved file', () => {
    const middle = `${'m'.repeat(20_000)}\n[Output truncated: showing 10 of 900 lines. Full output: ~/.octocode/out/web-1.txt; search it]\n${'n'.repeat(20_000)}`;
    const entries = [big(0, `HEAD ${'h'.repeat(5_000)}${middle}${'t'.repeat(5_000)} EXIT 1`), big(1, `${'y'.repeat(70_000)}\nLog: ~/.octocode/bash/bash-2-1.log`), ...filler(10)];
    const edits = planContextTrims(entries);
    const first = textOf(edits.find((edit) => edit.targetId === 'r0')!);
    expect(first.startsWith('HEAD ')).toBe(true);
    expect(first.endsWith(' EXIT 1')).toBe(true);
    expect(first).toMatch(/^\[Output truncated: showing 10 of 900 lines\. Full output: ~\/\.octocode\/out\/web-1\.txt; search it\]$/m);
    expect(first).toMatch(/full text is saved at ~\/\.octocode\/out\/web-1\.txt/);
    expect(first.length).toBeLessThan(1_500);
    // A pointer already in the tail is not repeated, and is still named in the note.
    const second = textOf(edits.find((edit) => edit.targetId === 'r1')!);
    expect(second.match(/Log: ~\/\.octocode\/bash\/bash-2-1\.log/g)).toHaveLength(1);
    expect(second).toMatch(/saved at ~\/\.octocode\/bash\/bash-2-1\.log/);
  });

  it('makes the file tool ask for a re-read once the latest read of a file is trimmed', async () => {
    const { fire, forget } = register();
    const entries = [
      readCall(0, 'read', { path: 'src/a.ts' }),
      big(0, 'a'.repeat(40_000), 'read'),
      readCall(1, 'mcp__octocode__localFetch', { queries: [{ path: '/repo/src/b.ts', fullContent: true }, { path: '/repo/src/c.ts', ranges: ['1-9'] }] }),
      big(1, 'b'.repeat(40_000), 'mcp__octocode__localFetch'),
      // d.ts is read again later, verbatim in the recent window: still in view.
      readCall(2, 'read', { path: 'src/d.ts' }),
      big(2, 'd'.repeat(40_000), 'read'),
      ...filler(10).slice(0, KEEP_RECENT_RESULTS - 1),
      readCall(99, 'read', { path: 'src/d.ts', offset: 1, limit: 5 }),
      big(99, 'short', 'read'),
    ];
    const result = (await fire('turn_end', turnEnd(entries))) as { entries: Array<{ targetId: string }> } | undefined;
    expect(result?.entries.map((edit) => edit.targetId)).toEqual(expect.arrayContaining(['r0', 'r1', 'r2']));
    expect(forget.mock.calls.map(([file]) => file).sort()).toEqual(['/repo/src/a.ts', '/repo/src/b.ts']);
  });
});
