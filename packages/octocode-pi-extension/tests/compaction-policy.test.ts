import fs from 'node:fs';
import path from 'node:path';
import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { idleContinuation } from '../src/compaction/cache.js';
import { registerCompaction } from '../src/compaction/register.js';
import type { FileGuard } from '../src/files/guard.js';

type Handler = (event: unknown, ctx: unknown) => Promise<unknown>;

afterEach(() => vi.restoreAllMocks());

describe('idle continuation', () => {
  const minute = 60_000;
  it('estimates from this session\'s own idle gaps whether the user returns before the entry expires', () => {
    const gaps = [2, 3, 4, 6, 8, 40, 50, 55, 70, 90].map((value) => value * minute);
    // Idle 1 minute, entry lives 5: of the 10 gaps past 1 minute, 4 end after the refresh would matter (1.5) and by 6.
    expect(idleContinuation(gaps, 1 * minute, 5 * minute)).toBeCloseTo(0.4);
    // Idle 30 minutes: 5 gaps lasted that long and none ended within 35.
    expect(idleContinuation(gaps, 30 * minute, 5 * minute)).toBe(0);
    // Too little history: no estimate (Pi's own decision stands).
    expect(idleContinuation([minute, minute], minute, 5 * minute)).toBeUndefined();
  });
});

describe('turn_end', () => {
  it('never edits the context of a turn the provider accepted, however large and old its results', async () => {
    const handlers = new Map<string, Handler>();
    const pi = { on: (name: string, handler: Handler) => void handlers.set(name, handler), registerProvider: vi.fn(), getSettings: () => ({}) } as unknown as ExtensionAPI;
    const guard = { restore: vi.fn(), forget: vi.fn() };
    registerCompaction(pi, guard as unknown as FileGuard);
    const model = { provider: 'p', id: 'm', api: 'anthropic-messages', promptCache: { short: 300, long: 3600 }, contextWindow: 200_000 };
    const ctx = {
      model,
      hasUI: true,
      cwd: '/repo',
      modelRegistry: { getAll: () => [model], find: () => model },
      sessionManager: { getSessionId: () => 's1', getBranch: () => [] },
      getContextUsage: () => ({ tokens: 190_000, contextWindow: 200_000, percent: 95 }),
    };
    const entries = Array.from({ length: 40 }, (_, index) => [
      { id: `a${index}`, message: { role: 'assistant', content: [{ type: 'toolCall', id: `c${index}`, name: 'file', arguments: { queries: [{ type: 'write', path: 'a.ts', content: 'z'.repeat(20_000) }] } }] } },
      { id: `r${index}`, message: { role: 'toolResult', toolName: 'read', toolCallId: `c${index}`, content: [{ type: 'text', text: 'o'.repeat(50_000) }, { type: 'image', data: 'A'.repeat(450_000), mimeType: 'image/png' }] } },
    ]).flat();
    const event = {
      entries: [],
      message: { role: 'assistant', stopReason: 'stop', provider: 'p', model: 'm', timestamp: Date.now(), usage: { cacheRead: 100_000, cacheWrite: 10 } },
      messageEntryId: 'last',
      toolResultEntryIds: [],
      context: { contextEntries: entries.map((entry) => ({ sourceEntry: { type: 'message', id: entry.id }, messages: [entry.message] })) },
    };
    expect(await handlers.get('turn_end')!(event, ctx)).toBeUndefined();
    expect(guard.forget).not.toHaveBeenCalled();
  });

  it('never warms the cache of a headless run or a subagent', async () => {
    const handlers = new Map<string, Handler>();
    const pi = { on: (name: string, handler: Handler) => void handlers.set(name, handler), registerProvider: vi.fn(), getSettings: () => ({}) } as unknown as ExtensionAPI;
    registerCompaction(pi, { restore: vi.fn(), forget: vi.fn() } as unknown as FileGuard);
    const decide = handlers.get('cache_warming_decision')!;
    const event = { type: 'cache_warming_decision', action: 'warm', warmCost: 0.01, missCost: 1, continuationProbability: 1 };
    expect(await decide(event, { hasUI: false, sessionManager: { getSessionId: () => 's', getBranch: () => [] } })).toEqual({ action: 'stop' });
    expect(await decide(event, { hasUI: true, sessionManager: { getSessionId: () => 's', getBranch: () => [] } })).toBeUndefined();
  });
});

/**
 * The session entries `context_edit` replaced old messages in 781 entries between Oct 3 and 13: the age-based trimming
 * that the `turn_end` handler of `compaction/register` ran on every turn until commit b8115f8c7 removed it. Since then
 * the only edit of an old message is the staged recovery from a request rejected as too large (HTTP 413).
 */
describe('context edits', () => {
  it('are produced only by the 413 recovery: no source file outside it names a context edit', () => {
    const sourceRoot = path.resolve(__dirname, '..', 'src');
    const walk = (dir: string): string[] => fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => (entry.isDirectory() ? walk(path.join(dir, entry.name)) : entry.name.endsWith('.ts') ? [path.join(dir, entry.name)] : []));
    const naming = walk(sourceRoot).filter((file) => /context_edit|ContextEdit|appendContextEdit/.test(fs.readFileSync(file, 'utf8'))).map((file) => path.relative(sourceRoot, file).split(path.sep).join('/'));
    expect(naming.sort()).toEqual(['compaction/images.ts', 'compaction/recovery.ts']);
  });

  it('are the only entries the turn_end handler returns, and only for a turn rejected as too large', async () => {
    const handlers = new Map<string, Handler>();
    const pi = { on: (name: string, handler: Handler) => void handlers.set(name, handler), registerProvider: vi.fn(), getSettings: () => ({}) } as unknown as ExtensionAPI;
    registerCompaction(pi, { restore: vi.fn(), forget: vi.fn() } as unknown as FileGuard);
    const model = { provider: 'p', id: 'm', api: 'anthropic-messages', promptCache: { short: 300, long: 3600 }, contextWindow: 200_000 };
    const ctx = { model, hasUI: false, cwd: '/repo', modelRegistry: { getAll: () => [model], find: () => model }, sessionManager: { getSessionId: () => 's1', getBranch: () => [] } };
    const picture = (index: number) => [
      { id: `a${index}`, message: { role: 'assistant', content: [{ type: 'toolCall', id: `c${index}`, name: 'read', arguments: { path: `/tmp/${index}.png` } }] } },
      { id: `r${index}`, message: { role: 'toolResult', toolName: 'read', toolCallId: `c${index}`, content: [{ type: 'text', text: 'img' }, { type: 'image', data: 'A'.repeat(1_000), mimeType: 'image/png' }] } },
    ];
    const entries = [0, 1, 2].flatMap(picture);
    const turn = (message: Record<string, unknown>) => ({
      entries: [],
      message: { role: 'assistant', content: [], timestamp: Date.now(), provider: 'p', model: 'm', usage: { cacheRead: 0, cacheWrite: 0 }, ...message },
      messageEntryId: 'last',
      toolResultEntryIds: [],
      context: { contextEntries: entries.map((entry) => ({ sourceEntry: { type: 'message', id: entry.id }, messages: [entry.message] })) },
    });
    // Large, old, image-heavy context and an accepted turn: nothing is returned, even long after the last request.
    expect(await handlers.get('turn_end')!(turn({ stopReason: 'stop' }), ctx)).toBeUndefined();
    expect(await handlers.get('turn_end')!(turn({ stopReason: 'error', errorMessage: 'connection reset' }), ctx)).toBeUndefined();
    const recovery = (await handlers.get('turn_end')!(turn({ stopReason: 'error', errorMessage: '413 status code (no body)' }), ctx)) as { entries: Array<{ type: string; targetId: string }> };
    expect(recovery.entries.length).toBeGreaterThan(0);
    expect(recovery.entries.every((entry) => entry.type === 'context_edit')).toBe(true);
    expect(new Set(recovery.entries.map((entry) => entry.targetId))).toEqual(new Set(['last', 'r0', 'r1']));
  });
});
