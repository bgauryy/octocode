import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cacheClock, idleContinuation, warmingDecision } from '../src/compaction/cache.js';
import { registerCompaction } from '../src/compaction/register.js';
import { CLEARED_RESULTS_TYPE, clearedResults, sessionCleared } from '../src/files/reads.js';
import type { FileGuard } from '../src/files/guard.js';
import { CACHE_RETENTION_ENV } from '../src/shared/env.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

/**
 * Building a request used to rescan the whole branch several times (the last cache use, the cleared results, the files
 * the context shows). The work is now proportional to the entries added since the last request: these tests count entry
 * visits, by a `type` that counts each read, never wall time.
 */

const T0 = 1_700_000_000_000;
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const model = { provider: 'p', id: 'm', api: 'anthropic-messages', promptCache: { short: 300, long: 3600 }, contextWindow: 200_000 };

let visits = 0;
/** `entry` whose `type` counts every read of it; the same object is returned by every `getBranch` call, as Pi's entries are. */
function counted<T extends { type: string }>(entry: T): T {
  const { type } = entry;
  return Object.defineProperty({ ...entry }, 'type', { get: () => (visits++, type), enumerable: true });
}

let next = 0;
const id = () => `e${next++}`;
const user = (at: number) => ({ type: 'message', id: id(), message: { role: 'user', content: 'go', timestamp: at } });
const assistant = (at: number, stopReason = 'stop') => ({ type: 'message', id: id(), message: { role: 'assistant', content: [], stopReason, timestamp: at } });
const warm = (at: number) => ({ type: 'usage', id: id(), kind: 'cache_warm', timestamp: new Date(at).toISOString() });
const clearedEntry = (...ids: string[]) => ({ type: 'custom', id: id(), customType: CLEARED_RESULTS_TYPE, data: { results: ids.map((each) => ({ id: each, saved: `/saved/${each}.txt` })) } });

/** `count` entries: runs of a prompt and its reply, with a refresh by the cache warmer and a cleared result now and then. */
function history(count: number, from = T0): Array<{ type: string; id: string }> {
  const entries: Array<{ type: string; id: string }> = [];
  let at = from;
  for (let index = 0; entries.length < count; index++) {
    at += 2 * MINUTE;
    entries.push(user(at), assistant(at + 1_000));
    if (index % 7 === 3) entries.push(warm(at + 2_000));
    if (index % 11 === 5) entries.push(clearedEntry(`call-${index}`));
  }
  return entries.slice(0, count);
}

const lastTimestamp = (entries: Array<Record<string, any>>) => Math.max(...entries.flatMap((entry) => (typeof entry['message']?.timestamp === 'number' ? [entry['message'].timestamp] : [])));

/** A session over a growing list of entries; `getBranch` copies the list, as Pi builds a new array for each call. */
function session(entries: unknown[]) {
  return { getSessionId: () => 's', getBranch: () => entries.slice(), getEntries: () => entries.slice(), buildSessionProjection: () => ({ entries: [], messages: [] }) };
}

let savedRetention: string | undefined;
beforeEach(() => {
  savedRetention = process.env[CACHE_RETENTION_ENV];
  process.env[CACHE_RETENTION_ENV] = 'long';
  process.env['OCTOCODE_HOME'] = tmp();
  visits = 0;
  next = 0;
});
afterEach(() => {
  vi.restoreAllMocks();
  if (savedRetention === undefined) delete process.env[CACHE_RETENTION_ENV];
  else process.env[CACHE_RETENTION_ENV] = savedRetention;
});

/** The `context` handler over a session with `entries`; `request(at)` is one model request made at `at`. */
function harness(entries: Array<{ type: string; id: string }>) {
  const fake = fakePi();
  registerCompaction(fake.pi, { restore: vi.fn(), forget: vi.fn() } as unknown as FileGuard);
  const ctx = fakeCtx({ cwd: '/repo', model });
  Object.assign(ctx, { sessionManager: session(entries) });
  const result = (callId: string) => ({ role: 'toolResult', toolCallId: callId, toolName: 'read', content: [{ type: 'text', text: 'the whole text' }], isError: false, timestamp: T0 });
  return {
    ctx,
    /** The text of the result `callId`, as the request carries it after the handler ran. */
    request: async (at: number, callId = 'call-5') => {
      vi.spyOn(Date, 'now').mockReturnValue(at);
      const messages = [result(callId)];
      const outcome = (await fake.fire('context', { type: 'context', messages }, ctx)) as { messages?: any[] } | undefined;
      return (outcome?.messages ?? messages)[0].content[0].text as string;
    },
  };
}

describe('the work of building a request grows with the entries added, not with the branch', () => {
  it('visits every entry once the first time, and none while the branch is unchanged (warm or cold cache)', async () => {
    const entries = history(1_000).map(counted);
    const s = harness(entries);
    const last = lastTimestamp(entries as never);
    expect(await s.request(last + 1_000)).toMatch(/^\[Old tool result cleared/);
    expect(visits).toBeGreaterThanOrEqual(1_000);
    visits = 0;
    // The cache is warm: nothing is cleared. Later, cold: nothing to clear either. Neither looks at an entry again.
    for (const at of [last + 2_000, last + 3 * HOUR, last + 4 * HOUR]) expect(await s.request(at)).toMatch(/^\[Old tool result cleared/);
    expect(visits).toBe(0);
  });

  it.each([1, 5, 50])('visits only the entries added when %i are appended to a 1,000-entry branch', async (added) => {
    const entries = history(1_000).map(counted);
    const s = harness(entries);
    const last = lastTimestamp(entries as never);
    await s.request(last + 1_000);
    visits = 0;
    const more: Array<{ type: string; id: string }> = [];
    for (let index = 0; index < added; index++) more.push(index % 2 === 0 ? user(last + (index + 1) * MINUTE) : assistant(last + (index + 1) * MINUTE));
    entries.push(...more.map(counted));
    await s.request(last + 3 * HOUR);
    expect(visits).toBeGreaterThan(0);
    // Each appended entry is read by the cache clock and the cleared results only: a small constant, whatever the branch holds.
    expect(visits).toBeLessThanOrEqual(added * 6);
  });

  it('a decision to warm the cache visits only what was added too', () => {
    const entries = history(1_000).map(counted);
    const ctx = { hasUI: true, sessionManager: session(entries) } as never;
    const event = { action: 'stop' as const, warmCost: 0.02, missCost: 0.5, continuationProbability: 0.15 };
    const last = lastTimestamp(entries as never);
    warmingDecision(event, ctx, HOUR, last + 5_000, false);
    expect(visits).toBeGreaterThanOrEqual(1_000);
    visits = 0;
    warmingDecision(event, ctx, HOUR, last + 10_000, false);
    expect(visits).toBe(0);
    entries.push(counted(user(last + 3 * MINUTE)));
    warmingDecision(event, ctx, HOUR, last + 3 * MINUTE, false);
    expect(visits).toBeLessThanOrEqual(6);
  });

  it('starts over when the branch changed under it (another leaf), and answers for the new branch', async () => {
    const shared = history(100);
    const branch = [...shared, clearedEntry('late')];
    const s = harness(branch);
    const last = lastTimestamp(shared as never);
    expect(await s.request(last + 1_000, 'late')).toMatch(/^\[Old tool result cleared/);
    // The session moved to a branch that forked before that entry: the result is not cleared there, the earlier ones still are.
    branch.splice(shared.length, 1, user(last + MINUTE));
    expect(await s.request(last + 2_000, 'late')).toBe('the whole text');
    expect(await s.request(last + 2_000, 'call-5')).toMatch(/^\[Old tool result cleared/);
    // And back to the first one, whose entry is a new object again.
    branch.splice(shared.length, 1, clearedEntry('late'));
    expect(await s.request(last + 3_000, 'late')).toMatch(/^\[Old tool result cleared/);
  });
});

/** The scans this replaces, kept as the oracle: what each answered for a whole branch. */
function oracleReply(branch: readonly any[]): number | undefined {
  for (let index = branch.length - 1; index >= 0; index--) {
    const message = branch[index].message;
    if (branch[index].type === 'message' && message?.role === 'assistant' && typeof message.timestamp === 'number') return message.timestamp;
  }
  return undefined;
}
function oracleGaps(branch: readonly any[]): number[] {
  const gaps: number[] = [];
  let finished: number | undefined;
  for (const entry of branch) {
    const message = entry.message;
    if (entry.type !== 'message' || typeof message?.timestamp !== 'number') continue;
    if (message.role === 'assistant') finished = message.stopReason === 'stop' ? message.timestamp : undefined;
    else if (message.role === 'user' && finished !== undefined) {
      gaps.push(message.timestamp - finished);
      finished = undefined;
    }
  }
  return gaps;
}
function oracleUse(branch: readonly any[]): number | undefined {
  const reply = oracleReply(branch);
  if (reply === undefined) return undefined;
  let latest = reply;
  for (const entry of branch) {
    const at = entry.type === 'usage' && entry.kind === 'cache_warm' && typeof entry.timestamp === 'string' ? Date.parse(entry.timestamp) : Number.NaN;
    if (at > latest) latest = at;
  }
  return latest;
}

describe('the incremental answers equal a scan of the whole branch', () => {
  it('for every prefix of a growing branch with replies that stopped early, odd entries and a switch of leaf', () => {
    const random = (() => {
      let seed = 7;
      return () => (seed = (seed * 48_271) % 2_147_483_647) / 2_147_483_647;
    })();
    const entries: any[] = [];
    const source = session(entries);
    let at = T0;
    const pick = (): any => {
      at += Math.floor(random() * 10 * MINUTE);
      const roll = random();
      if (roll < 0.3) return user(at);
      if (roll < 0.55) return assistant(at, 'stop');
      if (roll < 0.65) return assistant(at, 'toolUse');
      if (roll < 0.75) return warm(at + MINUTE);
      if (roll < 0.8) return { type: 'usage', id: id(), kind: 'cache_warm', timestamp: 'not a date' };
      if (roll < 0.85) return { type: 'message', id: id(), message: { role: 'user', content: 'no timestamp' } };
      if (roll < 0.9) return clearedEntry(`c${Math.floor(random() * 40)}`, `d${Math.floor(random() * 40)}`);
      if (roll < 0.93) return { type: 'custom', id: id(), customType: CLEARED_RESULTS_TYPE, data: { results: 'broken' } };
      return { type: 'label', id: id() };
    };
    for (let step = 0; step < 400; step++) {
      if (step === 250) entries.splice(120, entries.length - 120);
      entries.push(pick());
      if (step % 3 !== 0) continue;
      const clock = cacheClock(source);
      expect(clock.replyAt).toBe(oracleReply(entries));
      expect(clock.useAt).toBe(oracleUse(entries));
      expect([...clock.gaps]).toEqual(oracleGaps(entries));
      expect(new Map(sessionCleared(source))).toEqual(clearedResults(entries));
    }
    expect(idleContinuation(cacheClock(source).gaps, 0, HOUR)).toBe(idleContinuation(oracleGaps(entries), 0, HOUR));
  });

  it('for a branch whose entries have no ids: each call reads the whole branch again, correctly', () => {
    const entries: any[] = [{ type: 'message', message: { role: 'assistant', stopReason: 'stop', timestamp: 5 } }];
    const source = session(entries);
    expect(cacheClock(source).replyAt).toBe(5);
    entries.push({ type: 'message', message: { role: 'user', timestamp: 9 } }, { type: 'message', message: { role: 'assistant', stopReason: 'stop', timestamp: 12 } });
    expect(cacheClock(source).replyAt).toBe(12);
    expect([...cacheClock(source).gaps]).toEqual([4]);
  });
});
