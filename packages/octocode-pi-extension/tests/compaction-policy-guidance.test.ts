import assert from 'node:assert/strict';
import { test } from 'vitest';

import { registerCompactionPolicyGuidance } from '../src/tools/compaction-policy-guidance.js';
import type { PiContext, PiInstance } from '../src/types.js';

type Handler = (event: unknown, ctx: PiContext) => unknown | Promise<unknown>;

test('compaction policy warning is actionable, deduplicated, and never mutates settings', async () => {
  const handlers = new Map<string, Handler[]>();
  const notices: string[] = [];
  let reads = 0;
  const pi = {
    on(event: string, handler: Handler) {
      handlers.set(event, [...(handlers.get(event) ?? []), handler]);
    },
  } as unknown as PiInstance;
  registerCompactionPolicyGuidance(
    pi,
    (_ctx, message) => notices.push(message),
    () => {
      reads += 1;
      return { enabled: true, reserveTokens: 10_000 };
    },
  );
  const ctx = {
    cwd: '/workspace',
    getContextUsage: () => ({ tokens: 1_000, contextWindow: 100_000 }),
  } satisfies PiContext;

  await handlers.get('session_start')?.[0]?.({}, ctx);
  await handlers.get('model_select')?.[0]?.({}, ctx);

  assert.equal(reads, 2);
  assert.equal(notices.length, 1, 'same warning is shown once per session/model configuration');
  assert.match(notices[0] ?? '', /10,000.*20,000.*80%/i);
});

test('healthy compaction policy stays quiet', async () => {
  const handlers = new Map<string, Handler>();
  const notices: string[] = [];
  const pi = {
    on(event: string, handler: Handler) {
      handlers.set(event, handler);
    },
  } as unknown as PiInstance;
  registerCompactionPolicyGuidance(
    pi,
    (_ctx, message) => notices.push(message),
    () => ({ enabled: true, reserveTokens: 20_000 }),
  );
  const ctx = {
    getContextUsage: () => ({ tokens: 1_000, contextWindow: 100_000 }),
  } satisfies PiContext;

  await handlers.get('session_start')?.({}, ctx);
  assert.deepEqual(notices, []);
});
