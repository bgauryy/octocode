import assert from 'node:assert/strict';
import { test } from 'vitest';
import type {
  AutocompleteItem,
  CommandDefinition,
  PiCommandContext,
  PiImageContent,
  PiInstance,
  PiMessageContent,
  PiSendUserMessageOptions,
  PiSessionManager,
  PiTextContent,
  PiUi,
} from '../src/types.js';

test('Pi message content accepts text and base64 image parts', () => {
  const text: PiTextContent = { type: 'text', text: 'hello' };
  const image: PiImageContent = { type: 'image', data: 'aGVsbG8=', mimeType: 'image/png' };
  const content: PiMessageContent = [text, image];

  assert.deepEqual(content, [text, image]);
});

test('Pi UI dialog contract stays string-option and abort-signal based', async () => {
  const ui: PiUi = {
    select: async (_title, options, opts) => {
      opts?.signal?.throwIfAborted();
      return options[0];
    },
    input: async (_title, _placeholder, opts) => {
      opts?.signal?.throwIfAborted();
      return 'typed';
    },
  };

  assert.equal(await ui.select?.('Pick', ['one'], { signal: new AbortController().signal }), 'one');
  assert.equal(await ui.input?.('Prompt', 'placeholder', { timeout: 100 }), 'typed');
});

test('Pi session manager exposes readonly branch and tree accessors only', () => {
  const sessionManager: PiSessionManager = {
    getSessionFile: () => '/tmp/session.jsonl',
    getLeafId: () => 'entry-2',
    getBranch: () => [{ id: 'entry-1' }, { id: 'entry-2' }],
    getEntries: () => [{ id: 'entry-1' }, { id: 'entry-2' }],
    getTree: () => [{ entry: { id: 'entry-1' }, children: [] }],
  };

  assert.equal(sessionManager.getLeafId?.(), 'entry-2');
  assert.equal(sessionManager.getTree?.().length, 1);
});

test('Pi command context includes session navigation and injected rich user messages', async () => {
  const ctx: PiCommandContext = {
    sendUserMessage: (content, opts) => {
      assert.deepEqual(content, [{ type: 'text', text: 'next' }]);
      assert.equal(opts?.deliverAs, 'followUp');
      assert.equal(opts?.expandPromptTemplates, true);
    },
    switchSession: async (sessionPath, opts) => {
      assert.equal(sessionPath, '/tmp/session.jsonl');
      await opts?.withSession?.({});
      return { cancelled: false };
    },
    waitForIdle: async () => undefined,
  };

  ctx.sendUserMessage?.([{ type: 'text', text: 'next' }], { deliverAs: 'followUp', expandPromptTemplates: true });
  assert.deepEqual(await ctx.switchSession?.('/tmp/session.jsonl', { withSession: async () => undefined }), { cancelled: false });
  await ctx.waitForIdle?.();
});

test('Pi command completions may be asynchronous', async () => {
  const command: CommandDefinition = {
    description: 'async completion contract',
    handler: async () => undefined,
    getArgumentCompletions: async (_prefix): Promise<AutocompleteItem[] | null> => [
      { value: 'inspect ', label: 'inspect', description: 'Inspect state' },
    ],
  };

  assert.equal((await command.getArgumentCompletions?.('i'))?.[0]?.value, 'inspect ');
});

test('Pi instance custom messages accept rich content and delivery options', () => {
  const sent: Array<{ content: PiMessageContent; deliverAs?: string; expandPromptTemplates?: boolean }> = [];
  const pi: Pick<PiInstance, 'sendUserMessage' | 'sendMessage'> = {
    sendUserMessage: (content, opts) => {
      sent.push({ content, deliverAs: opts?.deliverAs, expandPromptTemplates: opts?.expandPromptTemplates });
    },
    sendMessage: (message, opts) => {
      sent.push({ content: message.content, deliverAs: opts?.deliverAs });
    },
  };

  const dispatchOptions: PiSendUserMessageOptions = { deliverAs: 'steer', expandPromptTemplates: true };
  pi.sendUserMessage([{ type: 'image', data: 'aGVsbG8=', mimeType: 'image/png' }], dispatchOptions);
  pi.sendMessage?.(
    { customType: 'octocode.test', content: [{ type: 'text', text: 'card' }], display: true },
    { triggerTurn: true, deliverAs: 'nextTurn' }
  );

  assert.deepEqual(sent.map((entry) => entry.deliverAs), ['steer', 'nextTurn']);
  assert.equal(sent[0]?.expandPromptTemplates, true);
});
