import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { describe, expect, it, vi } from 'vitest';
import { isRequestTooLarge, planTooLargeRecovery } from '../src/compaction/images.js';
import { registerCompaction } from '../src/compaction/register.js';
import type { FileGuard } from '../src/files/guard.js';

type Handler = (event: unknown, ctx: unknown) => Promise<unknown>;

/** A screenshot-sized image (about 450 KB of base64), as the `read` tool returns it. */
const SHOT = 'A'.repeat(450_000);
/** A result as Pi projects it once an image-stripping edit applied. */
const APPLIED = '[image from an earlier tool call omitted; saved at ~/x.png; read that file to see it again]';
const NOTE = expect.stringMatching(/^\[image from an earlier tool call omitted; saved at \S+\.png \(sha256 [0-9a-f]{16}, \d+ bytes\); read that file to see it again\]$/) as unknown as string;

const shot = (index: number, data = SHOT) => ({
  id: `r${index}`,
  message: {
    role: 'toolResult',
    toolName: 'read',
    toolCallId: `c${index}`,
    content: [
      { type: 'text', text: 'Read image file [image/png]' },
      { type: 'image', data, mimeType: 'image/png' },
    ],
  },
});
const readCall = (index: number) => ({
  id: `a${index}`,
  message: { role: 'assistant', content: [{ type: 'toolCall', id: `c${index}`, name: 'read', arguments: { path: `/tmp/shots/${index}.png` } }] },
});
const history = (count: number, data = SHOT) => Array.from({ length: count }, (_, index) => [readCall(index), shot(index, data)]).flat();
const failed = (text = '413 status code (no body)') => ({ role: 'assistant', stopReason: 'error', errorMessage: text, content: [], timestamp: Date.now() });
const turnEnd = (entries: Array<{ id: string; message: unknown }>, message: unknown, messageEntryId = 'fail', toolResultEntryIds: string[] = []) => ({
  entries: [],
  message,
  messageEntryId,
  toolResultEntryIds,
  context: { contextEntries: entries.map((entry) => ({ sourceEntry: { type: 'message', id: entry.id }, messages: [entry.message] })) },
});

function register(percent = 10) {
  const handlers = new Map<string, Handler>();
  const pi = { on: (name: string, handler: Handler) => void handlers.set(name, handler), getSettings: () => ({}) } as unknown as ExtensionAPI;
  const notify = vi.fn();
  registerCompaction(pi, { restore: vi.fn(), forget: vi.fn() } as unknown as FileGuard);
  const ctx = {
    model: { promptCache: { short: 300 } },
    cwd: '/repo',
    hasUI: true,
    ui: { notify },
    sessionManager: { getSessionId: () => 's1', getBranch: () => [], buildSessionProjection: () => ({ entries: [] }) },
    getContextUsage: () => ({ tokens: 1, contextWindow: 100, percent }),
  };
  return { notify, fire: (name: string, event: unknown = {}) => handlers.get(name)!(event, ctx) };
}

describe('request too large (HTTP 413)', () => {
  const ANTHROPIC = '413 {"error":{"type":"request_too_large","message":"Request exceeds the maximum size"}}';

  it('recognises byte-size rejections from Anthropic and from gateways without a body, not other errors', () => {
    expect(isRequestTooLarge(failed())).toBe(true);
    expect(isRequestTooLarge(failed(ANTHROPIC))).toBe(true);
    expect(isRequestTooLarge(failed('Request Entity Too Large'))).toBe(true);
    expect(isRequestTooLarge(failed('Payload Too Large'))).toBe(true);
    expect(isRequestTooLarge(failed('HTTP 413: rejected'))).toBe(true);
    expect(isRequestTooLarge(failed('429 rate limit'))).toBe(false);
    expect(isRequestTooLarge(failed('500 overloaded'))).toBe(false);
    // A bare number elsewhere in an error is not a status.
    expect(isRequestTooLarge(failed('prompt has 413 tokens too many'))).toBe(false);
    expect(isRequestTooLarge({ role: 'assistant', stopReason: 'stop', content: [{ type: 'text', text: 'HTTP 413 explained' }] })).toBe(false);
  });

  it('plans a recovery that omits the failed attempt and strips images beyond the ones kept', () => {
    const entries = [...history(3), { id: 'fail', message: failed() }];
    const keepOne = planTooLargeRecovery(entries, 'fail', 1)!;
    expect(keepOne.find((edit) => edit.targetId === 'fail')!.replacement).toBeNull();
    expect(keepOne.filter((edit) => edit.targetId !== 'fail').map((edit) => edit.targetId)).toEqual(['r0', 'r1']);
    const keepNone = planTooLargeRecovery(entries, 'fail', 0)!;
    expect(keepNone.filter((edit) => edit.targetId !== 'fail').map((edit) => edit.targetId)).toEqual(['r0', 'r1', 'r2']);
    // Without a failed id (Pi recovers that attempt itself) only images are stripped.
    expect(planTooLargeRecovery(entries, undefined, 1)!.map((edit) => edit.targetId)).toEqual(['r0', 'r1']);
    // Nothing left to strip: no recovery to try.
    expect(planTooLargeRecovery([readCall(0), { id: 'fail', message: failed() }], 'fail', 0)).toBeUndefined();
  });

  it('saves each stripped image and names the saved copy in its place, keeping the result text', () => {
    const edits = planTooLargeRecovery(history(2), undefined, 0)!;
    for (const edit of edits) {
      const content = (edit.replacement as { content: Array<{ type: string; text: string }> }).content;
      expect(content).toEqual([{ type: 'text', text: 'Read image file [image/png]' }, { type: 'text', text: NOTE }]);
      const shown = /saved at (\S+)/.exec(content[1]!.text)![1]!;
      const file = shown.startsWith('~') ? path.join(os.homedir(), shown.slice(2)) : shown;
      expect(fs.readFileSync(file).equals(Buffer.from(SHOT, 'base64'))).toBe(true);
    }
  });

  it('names every saved image and which could not be saved when only some of a message\'s images were', () => {
    const message = {
      role: 'toolResult',
      toolName: 'read',
      toolCallId: 'c0',
      content: [
        { type: 'text', text: 'two shots' },
        { type: 'image', data: SHOT, mimeType: 'image/png' },
        { type: 'image', data: undefined, mimeType: 'image/png' },
        { type: 'image', data: SHOT.slice(0, 1_000), mimeType: 'image/png' },
      ],
    };
    const edits = planTooLargeRecovery([{ id: 'r0', message }], undefined, 0)!;
    const content = (edits[0]!.replacement as { content: Array<{ text: string }> }).content;
    const note = content[1]!.text;
    const saved = [...note.matchAll(/(\S+\.png) \(sha256 [0-9a-f]{16}, \d+ bytes\)/g)].map((match) => match[1]!);
    expect(saved).toHaveLength(2);
    expect(note).toMatch(/^\[image from an earlier tool call omitted; saved at .*; image 2 of 3 could not be saved; read /);
    for (const shown of saved) expect(fs.existsSync(shown.startsWith('~') ? path.join(os.homedir(), shown.slice(2)) : shown)).toBe(true);
    // Nothing saved keeps the plain note.
    const none = planTooLargeRecovery([{ id: 'r1', message: { ...message, content: [{ type: 'image', data: undefined, mimeType: 'image/png' }] } }], undefined, 0)!;
    expect((none[0]!.replacement as { content: Array<{ text: string }> }).content[0]!.text).toBe('[image from an earlier tool call omitted; it could not be saved]');
  });

  it('strips images the user attached too, oldest first with the tool results, keeping the newest', () => {
    const pasted = { id: 'u0', message: { role: 'user', content: [{ type: 'text', text: 'see this' }, { type: 'image', data: SHOT, mimeType: 'image/png' }] } };
    const entries = [pasted, ...history(2), { id: 'fail', message: failed() }];
    const edits = planTooLargeRecovery(entries, 'fail', 1)!;
    expect(edits.filter((edit) => edit.targetId !== 'fail').map((edit) => edit.targetId)).toEqual(['u0', 'r0']);
    const content = (edits.find((edit) => edit.targetId === 'u0')!.replacement as { content: Array<{ text: string }> }).content;
    expect(content[0]!.text).toBe('see this');
    expect(content[1]!.text).toMatch(/^\[image from an earlier user message omitted; saved at /);
    // A text-only user message is never touched.
    expect(planTooLargeRecovery([{ id: 'u1', message: { role: 'user', content: 'hello' } }], undefined, 0)).toBeUndefined();
  });

  it('retries from agent_before_settle (Pi ignores turn_end continuation after an error): older images, then none, then stops', async () => {
    const { fire, notify } = register();
    const settle = (outcome = 'error') => fire('agent_before_settle', { type: 'agent_before_settle', outcome, entries: [], continue: false });
    const entries = history(3);
    const first = (await fire('turn_end', turnEnd(entries, failed()))) as { entries: Array<{ targetId: string; replacement: unknown }>; continue?: boolean };
    expect(first.continue).toBeUndefined();
    expect(first.entries.find((edit) => edit.targetId === 'fail')!.replacement).toBeNull();
    expect(first.entries.filter((edit) => edit.targetId !== 'fail').map((edit) => edit.targetId)).toEqual(['r0', 'r1']);
    expect(await settle()).toEqual({ continue: true });
    // One continuation per recovery.
    expect(await settle()).toBeUndefined();

    // Pi applied those edits; the retry still failed: strip the last image too.
    const projected = entries.map((entry) => (entry.id === 'r0' || entry.id === 'r1' ? { ...entry, message: { ...entry.message, content: [{ type: 'text', text: APPLIED }] } } : entry));
    const second = (await fire('turn_end', turnEnd(projected, failed(), 'fail2'))) as { entries: Array<{ targetId: string }> };
    expect(second.entries.map((edit) => edit.targetId)).toEqual(['fail2', 'r2']);
    expect(await settle()).toEqual({ continue: true });

    // No images left: no loop; the user is told to compact.
    const bare = projected.map((entry) => (entry.id === 'r2' ? { ...entry, message: { ...entry.message, content: [{ type: 'text', text: APPLIED }] } } : entry));
    expect(await fire('turn_end', turnEnd(bare, failed(), 'fail3'))).toBeUndefined();
    expect(await settle()).toBeUndefined();
    expect(notify).toHaveBeenCalledOnce();
    expect(notify.mock.calls[0]![0]).toMatch(/\/compact/);
    expect(notify.mock.calls[0]![1]).toBe('warning');
  });

  it('only strips images when Pi recognises the overflow itself (Pi compacts and retries)', async () => {
    const { fire, notify } = register();
    const result = (await fire('turn_end', turnEnd(history(3), failed(ANTHROPIC)))) as { entries: Array<{ targetId: string }> };
    expect(result.entries.map((edit) => edit.targetId)).toEqual(['r0', 'r1']);
    expect(await fire('agent_before_settle', { outcome: 'error', entries: [], continue: false })).toBeUndefined();
    expect(notify).not.toHaveBeenCalled();
  });

  it('does not continue a run that did not fail, and starts over after a successful turn or a tree move', async () => {
    const { fire } = register();
    await fire('turn_end', turnEnd(history(3), failed()));
    // The user aborted instead: no continuation.
    expect(await fire('agent_before_settle', { outcome: 'aborted', entries: [], continue: false })).toBeUndefined();
    await fire('turn_end', turnEnd(history(1), { role: 'assistant', stopReason: 'stop', content: [], timestamp: Date.now() }, 'ok'));
    const again = (await fire('turn_end', turnEnd(history(3), failed(), 'fail4'))) as { entries: Array<{ targetId: string }> };
    // First stage again: the newest image is kept.
    expect(again.entries.map((edit) => edit.targetId)).toEqual(['fail4', 'r0', 'r1']);
    await fire('session_tree', {});
    expect(await fire('agent_before_settle', { outcome: 'error', entries: [], continue: false })).toBeUndefined();
    const moved = (await fire('turn_end', turnEnd(history(3), failed(), 'fail5'))) as { entries: Array<{ targetId: string }> };
    expect(moved.entries.map((edit) => edit.targetId)).toEqual(['fail5', 'r0', 'r1']);
  });
});
