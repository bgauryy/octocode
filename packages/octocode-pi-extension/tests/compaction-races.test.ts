/** Compaction ownership and lifecycle regression tests. */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, test } from 'vitest';
import { Type } from 'typebox';
import { contentDigest } from '@octocodeai/octocode-awareness';
import type { CompactOptions, PiInstance, ToolDefinition } from '../src/types.js';
import { registerContextTools } from '../src/tools/context-tools.js';
import { __test__ as compactionInternals, registerCompactionHooks, resetCompactionCheckpointDedupe } from '../src/tools/compaction-hooks.js';
import { activePlanScope, clearPlan } from '../src/tools/active-plan.js';
import { buildCompactionMarkdown } from '../src/tools/compaction-artifacts.js';
import { createSessionArtifactContext, writeRehydrationLedger } from '../src/tools/session-artifacts.js';
import { consumeValidatedRehydration, rehydrateSession } from '../src/tools/rehydration-orchestrator.js';

type Handler = (event: unknown, ctx: unknown) => unknown | Promise<unknown>;

interface Harness {
  tools: Map<string, ToolDefinition>;
  notes: Array<{ msg: string; level?: string }>;
  sentUserMessages: Array<{ content: unknown; opts?: Record<string, unknown> }>;
  sentMessages: Array<{ customType?: string; content?: unknown; details?: unknown }>;
  handlerCount(event: string): number;
  fire(event: string, evt: unknown, ctx: unknown): Promise<unknown[]>;
}

function makeHarness(): Harness {
  const tools = new Map<string, ToolDefinition>();
  const handlers = new Map<string, Handler[]>();
  const notes: Array<{ msg: string; level?: string }> = [];
  const sentUserMessages: Array<{ content: unknown; opts?: Record<string, unknown> }> = [];
  const sentMessages: Array<{ customType?: string; content?: unknown; details?: unknown }> = [];
  const pi = {
    registerTool: (definition: ToolDefinition) => tools.set(definition.name, definition),
    registerCommand: () => undefined,
    sendUserMessage: (content: unknown, opts?: Record<string, unknown>) => sentUserMessages.push({ content, opts }),
    sendMessage: (message: { customType?: string; content?: unknown; details?: unknown }) => sentMessages.push(message),
    on: (event: string, handler: Handler) => {
      const current = handlers.get(event) ?? [];
      current.push(handler);
      handlers.set(event, current);
    },
  } as unknown as PiInstance;
  const notify = (_ctx: unknown, msg: string, level?: string) => notes.push({ msg, level });
  const registerFn = (
    target: { registerTool?(definition: ToolDefinition): void },
    names: Set<string>,
    definition: ToolDefinition,
  ) => {
    names.add(definition.name);
    target.registerTool?.(definition);
  };
  registerCompactionHooks(pi, notify as never);
  registerContextTools(pi, Type, new Set<string>(), registerFn as never, notify as never);
  return {
    tools,
    notes,
    sentUserMessages,
    sentMessages,
    handlerCount: (event) => handlers.get(event)?.length ?? 0,
    fire: (event, evt, ctx) => Promise.all((handlers.get(event) ?? []).map((handler) => handler(evt, ctx))),
  };
}

function makeCtx(options: { tokens?: number | null; contextWindow?: number; branch?: unknown[] } = {}) {
  const compactCalls: CompactOptions[] = [];
  const ctx = {
    hasUI: false,
    compact: (compactOptions: CompactOptions) => compactCalls.push(compactOptions),
    getContextUsage: () => ({
      tokens: options.tokens === undefined ? 90 : options.tokens,
      contextWindow: options.contextWindow ?? 100,
    }),
    sessionManager: {
      getBranch: () => options.branch ?? [],
      getSessionId: () => 'test-session',
    },
  };
  return { ctx, compactCalls };
}

let previousHome: string | undefined;
let testHome: string;

beforeEach(() => {
  previousHome = process.env['OCTOCODE_HOME'];
  testHome = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-compaction-races-home-'));
  process.env['OCTOCODE_HOME'] = testHome;
  resetCompactionCheckpointDedupe();
  clearPlan(activePlanScope());
  clearPlan(activePlanScope(makeCtx().ctx));
});

afterEach(() => {
  if (previousHome === undefined) delete process.env['OCTOCODE_HOME'];
  else process.env['OCTOCODE_HOME'] = previousHome;
  fs.rmSync(testHome, { recursive: true, force: true });
});

test('Octocode registers one proactive idle-boundary compactor and no turn_end abort race', () => {
  const harness = makeHarness();
  assert.equal(harness.handlerCount('agent_settled'), 1);
  assert.equal(harness.handlerCount('turn_end'), 0);
  assert.equal(harness.handlerCount('message_end'), 0);
  assert.equal(harness.tools.has('manage_context'), false);
});

test('completed turns compact once at 80 percent and ignore lower or unknown usage', async () => {
  const harness = makeHarness();
  const low = makeCtx({ tokens: 79, contextWindow: 100 });
  await harness.fire('agent_settled', {}, low.ctx);
  assert.deepEqual(low.compactCalls, []);

  const unknown = makeCtx({ tokens: null, contextWindow: 100 });
  await harness.fire('agent_settled', {}, unknown.ctx);
  assert.deepEqual(unknown.compactCalls, []);

  const { ctx, compactCalls } = makeCtx({ tokens: 80, contextWindow: 100 });
  await harness.fire('agent_settled', {}, ctx);
  await harness.fire('agent_settled', {}, ctx);
  assert.equal(compactCalls.length, 1, 'the in-flight guard prevents duplicate compaction');
  assert.match(compactCalls[0]?.customInstructions ?? '', /80%/);
});

test('failed proactive compaction can retry and completed compaction reports success', async () => {
  const harness = makeHarness();
  const { ctx, compactCalls } = makeCtx({ tokens: 90, contextWindow: 100 });
  await harness.fire('agent_settled', {}, ctx);
  compactCalls[0]?.onError?.(new Error('provider failed'));
  await harness.fire('agent_settled', {}, ctx);
  assert.equal(compactCalls.length, 2);
  compactCalls[1]?.onComplete?.({ tokensBefore: 90, estimatedTokensAfter: 30 });
  assert.ok(harness.notes.some((note) => /compaction completed: 90 → ~30 tokens; reclaimed ~60 \(67%\)/i.test(note.msg)));
  assert.equal(
    fs.existsSync(createSessionArtifactContext(ctx as never).resolve('compaction/metrics-latest.json')),
    true,
  );
});

test('Pi compaction failure events and synchronous host errors release the in-flight guard', async () => {
  const harness = makeHarness();
  const failed = makeCtx({ tokens: 90, contextWindow: 100 });
  await harness.fire('agent_settled', {}, failed.ctx);
  await harness.fire('session_compact_failed', {
    reason: 'threshold', aborted: false, willRetry: false, fromExtension: false, errorMessage: 'provider failed',
  }, failed.ctx);
  await harness.fire('agent_settled', {}, failed.ctx);
  assert.equal(failed.compactCalls.length, 2);
  failed.compactCalls[1]?.onComplete?.({ tokensBefore: 90, estimatedTokensAfter: 30 });

  const synchronous = makeCtx({ tokens: 90, contextWindow: 100 });
  let attempts = 0;
  Object.assign(synchronous.ctx, {
    compact: (options: CompactOptions) => {
      attempts += 1;
      if (attempts === 1) throw new Error('inactive host context');
      synchronous.compactCalls.push(options);
    },
  });
  await harness.fire('agent_settled', {}, synchronous.ctx);
  await harness.fire('agent_settled', {}, synchronous.ctx);
  assert.equal(attempts, 2);
  assert.equal(synchronous.compactCalls.length, 1);
  assert.ok(harness.notes.some((note) => /inactive host context/.test(note.msg)));
});

test('only public Pi compaction hooks are registered', () => {
  const harness = makeHarness();
  assert.equal(harness.handlerCount('session_before_compact'), 1);
  assert.equal(harness.handlerCount('session_compact'), 2);
  assert.equal(harness.handlerCount('session_compact_failed'), 1);
});

test('session shutdown discards staged smart-resume state without reading Pi stale context', async () => {
  const harness = makeHarness();
  const { ctx } = makeCtx();
  Object.assign(ctx, { cwd: testHome });
  const content = 'current memory bytes';
  writeRehydrationLedger(createSessionArtifactContext(ctx as never), {
    capturedAt: new Date().toISOString(),
    segments: [{
      version: 1,
      id: 'memory',
      kind: 'memory-lead',
      origin: 'memory',
      authority: 'external-data',
      digest: contentDigest(content),
      scope: 'task',
      visibility: 'inspectable',
      rehydrate: 'always',
    }],
    segmentContents: { memory: content },
    pendingInteractionIds: [],
    consumerCursors: {},
  });
  assert.equal(rehydrateSession(ctx as never, 'compaction', {
    getLivePlan: () => undefined,
    openContinuity: () => ({
      listPendingInteractions: () => [],
      getConsumerCursor: () => 0,
      close: () => undefined,
    }),
    setActivity: () => undefined,
  }).outcome, 'pending-validation');

  const staleReplacementCtx = new Proxy({}, {
    get() {
      throw new Error('replacement shutdown dereferenced stale Pi context');
    },
  });
  await harness.fire('session_shutdown', { reason: 'new' }, staleReplacementCtx);

  assert.equal(consumeValidatedRehydration(ctx as never, [{
    segment: {
      version: 1,
      id: 'memory',
      kind: 'memory-lead',
      origin: 'memory',
      authority: 'external-data',
      digest: contentDigest(content),
      scope: 'task',
      visibility: 'inspectable',
      rehydrate: 'always',
    },
    content,
  }], { allowProjection: true }), undefined);
});

test('successful overflow compaction checkpoints even when Pi will retry the interrupted turn', async () => {
  const harness = makeHarness();
  const { ctx } = makeCtx();
  await harness.fire('session_compact', {
    compactionEntry: { id: 'cmp-overflow', summary: 'Continue the interrupted task.' },
    fromExtension: false,
    reason: 'overflow',
    willRetry: true,
  }, ctx);
  assert.equal(
    harness.sentMessages.filter((message) => message.customType === 'octocode-compaction-checkpoint').length,
    1,
    'willRetry describes Pi retrying the agent turn; the compaction itself already succeeded',
  );
});

test('checkpoint card survives a smart-resume staging failure', async () => {
  const harness = makeHarness();
  const { ctx } = makeCtx();
  const missingWorkspace = path.join(testHome, 'does-not-exist');
  Object.assign(ctx, { cwd: missingWorkspace });
  await harness.fire('session_compact', {
    compactionEntry: { id: 'cmp-ledger-failure', summary: 'Continue from the active plan.' },
    fromExtension: false,
    reason: 'threshold',
    willRetry: false,
  }, ctx);
  assert.equal(
    harness.sentMessages.filter((message) => message.customType === 'octocode-compaction-checkpoint').length,
    1,
  );
  assert.match(harness.notes.at(-1)?.msg ?? '', /could not stage smart-resume metadata/i);
});

test('overflow fallback keeps resume, plan/doc references, files, and split-turn marker under pressure', () => {
  const build = compactionInternals.buildDeterministicCompaction as (
    preparation: Record<string, unknown>,
    reason: string,
    customInstructions: unknown,
    continuationContext?: string,
  ) => { summary: string } | null;
  const result = build({
    firstKeptEntryId: 'keep-1',
    tokensBefore: 120_000,
    previousSummary: 'previous '.repeat(2_000),
    messagesToSummarize: Array.from({ length: 20 }, () => ({ role: 'toolResult', content: 'history '.repeat(2_000) })),
    turnPrefixMessages: Array.from({ length: 20 }, () => ({ role: 'assistant', content: [{ type: 'text', text: 'split '.repeat(2_000) }] })),
    fileOps: { read: new Set(['docs/architecture.md']), edited: new Set(['src/compaction.ts']) },
  }, 'overflow', undefined, '<active_plan>rfc: docs/plan.md\nnext: verify recovery</active_plan>');
  assert.ok(result);
  assert.ok(result.summary.length <= 12_000);
  assert.match(result.summary, /## Resume instructions/);
  assert.match(result.summary, /docs\/plan\.md/);
  assert.match(result.summary, /src\/compaction\.ts/);
  assert.match(result.summary, /\*\*Turn Context \(split turn\):\*\*/);
});

test('extension compaction summaries and markdown redact credential-shaped text', () => {
  const build = compactionInternals.buildDeterministicCompaction as (
    preparation: Record<string, unknown>,
    reason: string,
    customInstructions: unknown,
  ) => { summary: string } | null;
  const fallback = build({
    firstKeptEntryId: 'keep-secret',
    tokensBefore: 100_000,
    messagesToSummarize: [{
      role: 'toolResult',
      content: [{ type: 'text', text: 'authorization: Bearer abcdefghijklmnopqrstuvwxyz password=hunter2' }],
    }],
    turnPrefixMessages: [],
    fileOps: {},
  }, 'overflow', 'api_key=sk-abcdefghijklmnopqrstuvwxyz');
  assert.ok(fallback);
  assert.doesNotMatch(fallback.summary, /hunter2|abcdefghijklmnopqrstuvwxyz/);
  assert.match(fallback.summary, /REDACTED/);

  const markdown = buildCompactionMarkdown({
    label: 'secret-check',
    summary: 'token=github_pat_abcdefghijklmnopqrstuvwxyz',
    reason: 'overflow',
  });
  assert.doesNotMatch(markdown, /github_pat_abcdefghijklmnopqrstuvwxyz/);
  assert.match(markdown, /token=\[REDACTED\]/);
});

test('manual compaction remains user-authoritative after a terminal answer', async () => {
  const harness = makeHarness();
  const { ctx } = makeCtx();
  const [result] = await harness.fire('session_before_compact', {
    preparation: {}, reason: 'manual', willRetry: false,
    branchEntries: [{
      type: 'message',
      message: { role: 'assistant', content: [{ type: 'text', text: 'The prior work was already complete, verified, and closed.' }] },
    }],
  }, ctx) as Array<{ cancel?: boolean } | undefined>;
  assert.equal(result, undefined);
  assert.equal(harness.notes.length, 0);
});

test('explicit manual compaction instructions are respected after a terminal answer', async () => {
  const harness = makeHarness();
  const { ctx } = makeCtx();
  const [result] = await harness.fire('session_before_compact', {
    preparation: {}, reason: 'manual', willRetry: false,
    customInstructions: 'Preserve the query audit details.',
    branchEntries: [{
      type: 'message',
      message: { role: 'assistant', content: [{ type: 'text', text: 'The prior work was already complete, verified, and closed.' }] },
    }],
  }, ctx);
  assert.equal(result, undefined);
});

test('Pi and user compactions never schedule an Octocode follow-up turn', async () => {
  const harness = makeHarness();
  const { ctx } = makeCtx();
  await harness.fire('session_compact', {
    compactionEntry: {}, fromExtension: false, reason: 'threshold', willRetry: false,
  }, ctx);
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(harness.sentUserMessages, []);
});

test('checkpoint-card dedupe resets stable-id and object-identity paths', async () => {
  const harness = makeHarness();
  const { ctx } = makeCtx();
  const count = () => harness.sentMessages.filter((message) => message.customType === 'octocode-compaction-checkpoint').length;

  await harness.fire('session_compact', { compactionEntry: { id: 'cmp-abc' }, reason: 'threshold', willRetry: false }, ctx);
  await harness.fire('session_compact', { compactionEntry: { id: 'cmp-abc' }, reason: 'threshold', willRetry: false }, ctx);
  assert.equal(count(), 1);
  resetCompactionCheckpointDedupe();
  await harness.fire('session_compact', { compactionEntry: { id: 'cmp-abc' }, reason: 'threshold', willRetry: false }, ctx);
  assert.equal(count(), 2);

  const entry = { reason: 'threshold' };
  await harness.fire('session_compact', { compactionEntry: entry, reason: 'threshold', willRetry: false }, ctx);
  await harness.fire('session_compact', { compactionEntry: entry, reason: 'threshold', willRetry: false }, ctx);
  assert.equal(count(), 3);
  resetCompactionCheckpointDedupe();
  await harness.fire('session_compact', { compactionEntry: entry, reason: 'threshold', willRetry: false }, ctx);
  assert.equal(count(), 4);
});
