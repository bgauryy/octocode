import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  OUTPUT_LIMIT_RECOVERY_PROMPT,
  registerModelOutputRecovery,
} from '../src/tools/model-output-recovery.js';
import type { PiInstance } from '../src/types.js';

type Handler = (event: unknown, ctx: unknown) => unknown | Promise<unknown>;

function harness() {
  const handlers = new Map<string, Handler[]>();
  const followUps: Array<{ content: unknown; opts?: Record<string, unknown> }> = [];
  const notices: Array<{ message: string; level?: string }> = [];
  const pi = {
    on(event: string, handler: Handler) {
      handlers.set(event, [...(handlers.get(event) ?? []), handler]);
    },
    sendUserMessage(content: unknown, opts?: Record<string, unknown>) {
      followUps.push({ content, opts });
    },
  } as unknown as PiInstance;
  registerModelOutputRecovery(pi, (_ctx, message, level) => notices.push({ message, level }));
  return {
    followUps,
    notices,
    fire: async (event: string, payload: unknown) => {
      await Promise.all((handlers.get(event) ?? []).map((handler) => handler(payload, {})));
      await new Promise<void>((resolve) => queueMicrotask(resolve));
    },
  };
}

function lengthMessage() {
  return { role: 'assistant', content: [{ type: 'text', text: 'partial' }], stopReason: 'length' };
}

test('maximum-output stop retries once with bounded continuation guidance', async () => {
  const h = harness();
  await h.fire('agent_end', { messages: [lengthMessage()], willRetry: false });

  assert.deepEqual(h.followUps, [{
    content: OUTPUT_LIMIT_RECOVERY_PROMPT,
    opts: { deliverAs: 'followUp' },
  }]);
  assert.match(h.notices[0]?.message ?? '', /retrying once.*smaller/i);
});

test('maximum-output recovery never loops and ignores Pi overflow retries', async () => {
  const h = harness();
  const overflow = lengthMessage();
  await h.fire('agent_end', { messages: [overflow], willRetry: true });
  assert.equal(h.followUps.length, 0, 'Pi owns context-overflow retry');

  const first = lengthMessage();
  await h.fire('agent_end', { messages: [first], willRetry: false });
  await h.fire('agent_end', { messages: [first], willRetry: false });
  assert.equal(h.followUps.length, 1, 'duplicate lifecycle delivery is idempotent');

  await h.fire('agent_end', { messages: [lengthMessage()], willRetry: false });
  assert.equal(h.followUps.length, 1, 'a second truncated retry stops automatically');
  assert.match(h.notices.at(-1)?.message ?? '', /automatic retry stopped/i);

  await h.fire('input', { source: 'interactive', text: 'continue in smaller steps' });
  await h.fire('agent_end', { messages: [lengthMessage()], willRetry: false });
  assert.equal(h.followUps.length, 2, 'fresh user input re-arms one recovery attempt');
});

test('a normal completion re-arms output-limit recovery', async () => {
  const h = harness();
  await h.fire('agent_end', { messages: [lengthMessage()], willRetry: false });
  await h.fire('agent_end', {
    messages: [{ role: 'assistant', content: [{ type: 'text', text: 'done' }], stopReason: 'stop' }],
    willRetry: false,
  });
  await h.fire('agent_end', { messages: [lengthMessage()], willRetry: false });
  assert.equal(h.followUps.length, 2);
});
