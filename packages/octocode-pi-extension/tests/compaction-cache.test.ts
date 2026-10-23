import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { INFERRED_CACHE_TTL_SECONDS, PromptCacheLifetimes, inferredPromptCache, promptCacheTtlMs, warmingDecision } from '../src/compaction/cache.js';
import { contextChars } from '../src/compaction/images.js';
import { registerCompaction } from '../src/compaction/register.js';
import type { FileGuard } from '../src/files/guard.js';
import { CACHE_RETENTION_ENV, childCacheRetention, chooseSessionCacheRetention } from '../src/shared/env.js';
import { subagentProcessEnv } from '../src/subagents/process.js';

type Handler = (event: unknown, ctx: unknown) => Promise<unknown>;
type TestModel = { provider: string; id: string; api: string; promptCache?: { short?: number; long?: number }; compat?: Record<string, unknown>; headers?: Record<string, string> };

const CHOSEN = Symbol.for('octocode.cacheRetentionChosen');
const forgetChoice = () => void delete (globalThis as Record<symbol, unknown>)[CHOSEN];

afterEach(() => {
  forgetChoice();
  vi.restoreAllMocks();
});

const opus = (extra: Partial<TestModel> = {}): TestModel => ({ provider: 'gateway', id: 'claude-opus', api: 'anthropic-messages', ...extra });
const reply = (model: TestModel, usage: Record<string, number>, extra: Record<string, unknown> = {}) => ({
  role: 'assistant',
  provider: model.provider,
  model: model.id,
  api: model.api,
  timestamp: Date.now(),
  content: [],
  usage: { input: 4, output: 100, cacheRead: 0, cacheWrite: 0, ...usage },
  stopReason: 'toolUse',
  ...extra,
});

/** A Pi double whose registry composes `registerProvider` models the way Pi does: a registration replaces the provider's models. */
function fakePi(models: TestModel[]) {
  const handlers = new Map<string, Handler>();
  let catalog = [...models];
  const registerProvider = vi.fn((name: string, config: { models: TestModel[] }) => {
    catalog = [...catalog.filter((model) => model.provider !== name), ...config.models.map((model) => ({ ...model, provider: name }))];
  });
  const pi = { on: (name: string, handler: Handler) => void handlers.set(name, handler), registerProvider, getSettings: () => ({}) } as unknown as ExtensionAPI;
  const current = { model: models[0] };
  const usage = { percent: 12 as number | null };
  const ctx = {
    get model() {
      return catalog.find((model) => model.provider === current.model?.provider && model.id === current.model?.id);
    },
    hasUI: true,
    cwd: '/repo',
    modelRegistry: { getAll: () => catalog, find: (provider: string, id: string) => catalog.find((model) => model.provider === provider && model.id === id) },
    sessionManager: { getSessionId: () => 's1', getBranch: () => [], getHeader: () => null },
    getContextUsage: () => ({ tokens: 1, contextWindow: 1_000_000, percent: usage.percent }),
  };
  return { pi, ctx, handlers, registerProvider, usage, catalog: () => catalog, fire: (name: string, event: unknown = {}) => handlers.get(name)!(event, ctx) };
}

const turnEnd = (entries: Array<{ id: string; message: unknown }>, message: unknown) => ({
  entries: [],
  message,
  toolResultEntryIds: [],
  context: { contextEntries: entries.map((entry) => ({ sourceEntry: { type: 'message', id: entry.id }, messages: [entry.message] })) },
});

function compaction(models: TestModel[]) {
  const pi = fakePi(models);
  registerCompaction(pi.pi, { restore: vi.fn(), forget: vi.fn() } as unknown as FileGuard);
  return pi;
}

describe('request size', () => {
  it('counts what the provider is sent: text, thinking with its signature, tool-call arguments and images', () => {
    const entries = [
      { id: 'a', message: { role: 'assistant', content: [{ type: 'thinking', thinking: 'abc', thinkingSignature: 'sig45' }, { type: 'toolCall', id: 'c', name: 'bash', arguments: { command: 'ls' } }] } },
      { id: 'r', message: { role: 'toolResult', content: [{ type: 'text', text: 'hello' }, { type: 'image', data: 'QUJD', mimeType: 'image/png' }] } },
      { id: 'u', message: { role: 'user', content: 'hi' } },
    ];
    const args = { command: 'x'.repeat(10_000) };
    const withArgs = [{ id: 'b', message: { role: 'assistant', content: [{ type: 'toolCall', id: 'd', name: 'file', arguments: args }] } }];
    // Thinking text, its signature, the call, the text, the image and the user text all count.
    expect(contextChars(entries)).toBeGreaterThanOrEqual(3 + 5 + 'bash'.length + 'command'.length + 'ls'.length + 5 + 4 + 2);
    expect(contextChars(withArgs)).toBeGreaterThanOrEqual(10_000);
    expect(contextChars(withArgs)).toBeLessThanOrEqual(JSON.stringify(args).length + 'file'.length + 8);
  });
});

describe('prompt-cache lifetimes for models that declare none', () => {
  it('infers the Anthropic lifetimes, and the short one for long retention where the model cannot hold a long entry', () => {
    expect(inferredPromptCache({})).toEqual({ short: INFERRED_CACHE_TTL_SECONDS.short, long: INFERRED_CACHE_TTL_SECONDS.long });
    expect(inferredPromptCache({ compat: { supportsLongCacheRetention: false } })).toEqual({ short: 300, long: 300 });
    expect(promptCacheTtlMs({ short: 300, long: 3600 }, {})).toBe(300_000);
    expect(promptCacheTtlMs({ short: 300, long: 3600 }, { [CACHE_RETENTION_ENV]: 'long' })).toBe(3_600_000);
    expect(promptCacheTtlMs({ long: 3600 }, {})).toBeUndefined();
    expect(promptCacheTtlMs(undefined, {})).toBeUndefined();
  });

  it('gives an anthropic-messages model that reports cache use lifetimes in Pi\'s registry, so Pi warms its cache', async () => {
    const model = opus();
    const sibling = opus({ id: 'claude-haiku' });
    const session = compaction([model, sibling]);
    // No cache activity yet: nothing is invented.
    await session.fire('turn_end', turnEnd([], reply(model, {})));
    expect(session.registerProvider).not.toHaveBeenCalled();
    await session.fire('turn_end', turnEnd([], reply(model, { cacheWrite: 9_000 })));
    expect(session.registerProvider).toHaveBeenCalledOnce();
    const [name, config] = session.registerProvider.mock.calls[0]!;
    expect(name).toBe('gateway');
    // Every model of the provider stays; only the one that showed caching gains lifetimes, the rest of it unchanged.
    expect(config.models).toEqual([{ ...model, promptCache: { short: 300, long: 3600 } }, sibling]);
    expect(session.ctx.model?.promptCache).toEqual({ short: 300, long: 3600 });
    // Once supplied, later replies register nothing again.
    await session.fire('turn_end', turnEnd([], reply(model, { cacheRead: 9_000 })));
    expect(session.registerProvider).toHaveBeenCalledOnce();
  });

  it('never supplies lifetimes for a model that declares its own, another API, or a provider whose models carry headers', async () => {
    const declared = compaction([opus({ promptCache: { short: 600 } })]);
    await declared.fire('turn_end', turnEnd([], reply(opus(), { cacheRead: 5 })));
    const openai = opus({ api: 'openai-completions' });
    const other = compaction([openai]);
    await other.fire('turn_end', turnEnd([], reply(openai, { cacheRead: 5 })));
    const headed = opus({ headers: { 'x-team': 'a' } });
    const withHeaders = compaction([headed]);
    await withHeaders.fire('turn_end', turnEnd([], reply(headed, { cacheRead: 5 })));
    for (const session of [declared, other, withHeaders]) expect(session.registerProvider).not.toHaveBeenCalled();
  });

  it('keeps the inferred lifetime for idle-warming decisions when the registry refuses the registration, and does not retry it every turn', () => {
    const model = opus();
    const pi = fakePi([model]);
    pi.registerProvider.mockImplementation(() => {
      throw new Error('refused');
    });
    const lifetimes = new PromptCacheLifetimes();
    lifetimes.observe(pi.pi, pi.ctx as never, reply(model, { cacheRead: 5 }));
    lifetimes.observe(pi.pi, pi.ctx as never, reply(model, { cacheRead: 5 }));
    expect(pi.registerProvider).toHaveBeenCalledOnce();
    expect(lifetimes.of(model as never)).toEqual({ short: 300, long: 3600 });
    expect(lifetimes.of(opus({ id: 'never-cached' }) as never)).toBeUndefined();
  });
});

describe('granted tier and warming', () => {
  it('learns from cacheWrite1h that the provider granted only 5 minutes, and supplies a 300 s long tier', () => {
    const model = opus();
    const pi = fakePi([model]);
    const lifetimes = new PromptCacheLifetimes();
    const long = { [CACHE_RETENTION_ENV]: 'long' };
    lifetimes.observe(pi.pi, pi.ctx as never, reply(model, { cacheWrite: 900, cacheWrite1h: 900 }), long);
    expect(pi.ctx.model?.promptCache).toEqual({ short: 300, long: 3600 });
    lifetimes.observe(pi.pi, pi.ctx as never, reply(model, { cacheWrite: 900, cacheWrite1h: 0 }), long);
    expect(pi.registerProvider).toHaveBeenCalledTimes(2);
    expect(pi.ctx.model?.promptCache).toEqual({ short: 300, long: 300 });
    expect(lifetimes.of(pi.ctx.model as never)).toEqual({ short: 300, long: 300 });
    // Learned once: later replies change nothing.
    lifetimes.observe(pi.pi, pi.ctx as never, reply(model, { cacheWrite: 900, cacheWrite1h: 0 }), long);
    expect(pi.registerProvider).toHaveBeenCalledTimes(2);
  });

  it('decides an idle refresh from the session\'s own idle gaps, and leaves streaming refreshes to Pi', () => {
    const minute = 60_000;
    const now = 100 * 60 * minute;
    // Ten runs, each followed by a 3-minute pause, and the last reply 1 minute ago.
    const branch = Array.from({ length: 10 }, (_, index) => [
      { type: 'message', message: { role: 'assistant', stopReason: 'stop', timestamp: index * 10 * minute } },
      { type: 'message', message: { role: 'user', timestamp: index * 10 * minute + 3 * minute } },
    ]).flat();
    branch.push({ type: 'message', message: { role: 'assistant', stopReason: 'stop', timestamp: now - minute } });
    const ctx = { hasUI: true, sessionManager: { getBranch: () => branch } } as never;
    const idle = { action: 'stop' as const, warmCost: 0.02, missCost: 0.5, continuationProbability: 0.15 };
    // Every pause ended between 1.5 and 6 minutes: q = 1, so a 5-minute entry is worth refreshing.
    expect(warmingDecision(idle, ctx, 5 * minute, now)).toEqual({ action: 'warm' });
    // An hour-long entry outlives every pause without a refresh this early: the expected return is before 0.1τ.
    expect(warmingDecision({ ...idle, action: 'warm' }, ctx, 60 * minute, now)).toEqual({ action: 'stop' });
    expect(warmingDecision({ ...idle, continuationProbability: 1 }, ctx, 5 * minute, now)).toBeUndefined();
    expect(warmingDecision({ ...idle, action: 'warm' }, { hasUI: false, sessionManager: { getBranch: () => branch } } as never, 5 * minute, now)).toEqual({ action: 'stop' });
    // A subagent reports a UI over RPC but nobody idles at it: it never keeps a cache warm, by argument or by its env.
    expect(warmingDecision({ ...idle, action: 'warm' }, ctx, 5 * minute, now, true)).toEqual({ action: 'stop' });
    vi.stubEnv('OCTOCODE_SUBAGENT', '1');
    try {
      expect(warmingDecision({ ...idle, action: 'warm' }, ctx, 5 * minute, now)).toEqual({ action: 'stop' });
    } finally {
      vi.unstubAllEnvs();
    }
  });
});

describe('cache retention', () => {
  it('uses long retention for an interactive main session and short for headless runs, unless the user chose one', () => {
    const ui: NodeJS.ProcessEnv = {};
    chooseSessionCacheRetention(true, ui);
    expect(ui[CACHE_RETENTION_ENV]).toBe('long');
    forgetChoice();
    const headless: NodeJS.ProcessEnv = {};
    chooseSessionCacheRetention(false, headless);
    expect(headless[CACHE_RETENTION_ENV]).toBe('short');
    forgetChoice();
    const user: NodeJS.ProcessEnv = { [CACHE_RETENTION_ENV]: 'short' };
    chooseSessionCacheRetention(true, user);
    expect(user[CACHE_RETENTION_ENV]).toBe('short');
  });

  it('runs subagents on short retention unless the user exported one; the main session\'s own long never leaks', () => {
    const main: NodeJS.ProcessEnv = {};
    chooseSessionCacheRetention(true, main);
    expect(childCacheRetention(main)).toBe('short');
    expect(subagentProcessEnv(undefined, main)[CACHE_RETENTION_ENV]).toBe('short');
    // A second session in the same process (a /new or /reload) sees Octocode's choice, not a user value.
    chooseSessionCacheRetention(true, main);
    expect(subagentProcessEnv(undefined, main)[CACHE_RETENTION_ENV]).toBe('short');
    forgetChoice();
    const exported: NodeJS.ProcessEnv = { [CACHE_RETENTION_ENV]: 'long' };
    expect(subagentProcessEnv(undefined, exported)[CACHE_RETENTION_ENV]).toBe('long');
    expect(subagentProcessEnv(undefined, {})[CACHE_RETENTION_ENV]).toBe('short');
  });

  it('chooses retention at session start, before the first request, so pi-ai and Pi\'s warmer use one lifetime', async () => {
    const saved = process.env[CACHE_RETENTION_ENV];
    delete process.env[CACHE_RETENTION_ENV];
    try {
      const session = compaction([opus()]);
      await session.fire('session_start', { type: 'session_start', reason: 'startup' });
      expect(process.env[CACHE_RETENTION_ENV]).toBe('long');
      await session.fire('turn_end', turnEnd([], reply(opus(), { cacheRead: 5 })));
      // Pi's warmer reads the supplied tier for the same retention (cache-warmer.js getPromptCacheTtlMs).
      expect(promptCacheTtlMs(session.ctx.model?.promptCache)).toBe(3_600_000);
      // A process that lost the value (loaded by an earlier build, a reload during a rebuild) gets it back before its
      // next run, without waiting for a new session.
      delete process.env[CACHE_RETENTION_ENV];
      await session.fire('before_agent_start', { type: 'before_agent_start', prompt: 'next' });
      expect(process.env[CACHE_RETENTION_ENV]).toBe('long');
    } finally {
      if (saved === undefined) delete process.env[CACHE_RETENTION_ENV];
      else process.env[CACHE_RETENTION_ENV] = saved;
    }
  });
});
