/**
 * Tests for desktop-notify: enable gating, OSC 9 sanitization/cap, title-flash
 * timer behavior, and the shutdown suppress flag.
 */
import assert from 'node:assert/strict';
import { test, beforeEach } from 'vitest';
import {
  OSC9_MAX_CHARS,
  clearTitleFlashTimer,
  desktopNotificationsSuppressed,
  emitOsc9,
  flashTerminalTitle,
  isTitleFlashPendingForTests,
  notificationsEnabled,
  resumeDesktopNotificationsForTests,
  suppressDesktopNotifications,
} from '../src/tools/desktop-notify.js';
import type { PiContext } from '../src/types.js';

beforeEach(() => {
  resumeDesktopNotificationsForTests();
  clearTitleFlashTimer();
});

// ─── notificationsEnabled ─────────────────────────────────────────────────────

test('notificationsEnabled: truthy OCTOCODE_NOTIFY forces on even without a TTY', () => {
  assert.equal(notificationsEnabled({ OCTOCODE_NOTIFY: '1' } as NodeJS.ProcessEnv), true);
  assert.equal(notificationsEnabled({ OCTOCODE_NOTIFY: 'true' } as NodeJS.ProcessEnv), true);
  assert.equal(notificationsEnabled({ OCTOCODE_NOTIFY: 'yes' } as NodeJS.ProcessEnv), true);
});

test('notificationsEnabled: explicit opt-out values win', () => {
  assert.equal(notificationsEnabled({ OCTOCODE_NOTIFY: '0' } as NodeJS.ProcessEnv), false);
  assert.equal(notificationsEnabled({ OCTOCODE_NOTIFY: 'false' } as NodeJS.ProcessEnv), false);
  assert.equal(notificationsEnabled({ OCTOCODE_NOTIFY: 'FALSE' } as NodeJS.ProcessEnv), false);
  assert.equal(notificationsEnabled({ OCTOCODE_NOTIFY: '' } as NodeJS.ProcessEnv), false);
});

test('notificationsEnabled: unset falls back to stdout TTY detection', () => {
  const expected = process.stdout?.isTTY === true;
  assert.equal(notificationsEnabled({} as NodeJS.ProcessEnv), expected);
});

// ─── emitOsc9 ─────────────────────────────────────────────────────────────────

function collectWrites(): { writes: string[]; write: (s: string) => void } {
  const writes: string[] = [];
  return { writes, write: (s) => writes.push(s) };
}

test('emitOsc9 wraps the message in an OSC 9 sequence', () => {
  const { writes, write } = collectWrites();
  emitOsc9('worker done', write);
  assert.deepEqual(writes, ['\x1b]9;worker done\x07']);
});

test('emitOsc9 strips control chars, BEL, and embedded escapes from the payload', () => {
  const { writes, write } = collectWrites();
  emitOsc9('a\x07b\nc\x1b]0;evil\x07d\rE', write);
  assert.equal(writes.length, 1);
  const payload = writes[0]!.slice('\x1b]9;'.length, -1);
  assert.ok(!payload.includes('\x07'), 'BEL must not appear inside the payload');
  assert.ok(!payload.includes('\x1b'), 'ESC must not appear inside the payload');
  assert.ok(!/[\n\r]/.test(payload), 'newlines must not appear inside the payload');
  assert.ok(payload.includes('a') && payload.includes('E'));
  // Sequence still well-formed: single OSC 9 with a single terminating BEL.
  assert.ok(writes[0]!.startsWith('\x1b]9;') && writes[0]!.endsWith('\x07'));
});

test('emitOsc9 caps the payload at OSC9_MAX_CHARS with an ellipsis', () => {
  const { writes, write } = collectWrites();
  emitOsc9('x'.repeat(500), write);
  const payload = writes[0]!.slice('\x1b]9;'.length, -1);
  assert.equal(payload.length, OSC9_MAX_CHARS);
  assert.ok(payload.endsWith('…'));
});

test('emitOsc9 emits nothing for empty / whitespace-only / control-only messages', () => {
  const { writes, write } = collectWrites();
  emitOsc9('', write);
  emitOsc9('   ', write);
  emitOsc9('\x07\x07', write);
  assert.deepEqual(writes, []);
});

test('emitOsc9 is a no-op after suppressDesktopNotifications', () => {
  const { writes, write } = collectWrites();
  suppressDesktopNotifications();
  assert.equal(desktopNotificationsSuppressed(), true);
  emitOsc9('late teardown event', write);
  assert.deepEqual(writes, []);
  resumeDesktopNotificationsForTests();
  emitOsc9('after resume', write);
  assert.equal(writes.length, 1);
});

// ─── flashTerminalTitle ───────────────────────────────────────────────────────

function makeTitleCtx(): { ctx: PiContext; titles: string[] } {
  const titles: string[] = [];
  const ctx = {
    hasUI: true,
    ui: { setTitle: (t: string) => { titles.push(t); } },
  } as unknown as PiContext;
  return { ctx, titles };
}

test('flashTerminalTitle sets a flagged title and restores it after the timer', async () => {
  const { ctx, titles } = makeTitleCtx();
  flashTerminalTitle(ctx, 'worker alpha finished', 'Octocode', 10);
  assert.equal(titles.length, 1);
  assert.ok(titles[0]!.includes('worker alpha finished'));
  assert.equal(isTitleFlashPendingForTests(), true);
  await new Promise((r) => setTimeout(r, 40));
  assert.deepEqual(titles.length, 2);
  assert.equal(titles[1], 'Octocode');
  assert.equal(isTitleFlashPendingForTests(), false);
});

test('clearTitleFlashTimer cancels the pending restore', async () => {
  const { ctx, titles } = makeTitleCtx();
  flashTerminalTitle(ctx, 'flash', 'Octocode', 10);
  assert.equal(isTitleFlashPendingForTests(), true);
  clearTitleFlashTimer();
  assert.equal(isTitleFlashPendingForTests(), false);
  await new Promise((r) => setTimeout(r, 30));
  assert.equal(titles.length, 1, 'restore must not fire after clear');
});

test('a newer flash supersedes the pending restore of an older one', async () => {
  const { ctx, titles } = makeTitleCtx();
  flashTerminalTitle(ctx, 'first', 'Octocode', 10);
  flashTerminalTitle(ctx, 'second', 'Octocode', 10);
  assert.equal(titles.length, 2); // two flash sets, no restore between them
  await new Promise((r) => setTimeout(r, 40));
  assert.equal(titles.length, 3, 'exactly one restore fires');
  assert.equal(titles[2], 'Octocode');
});

test('flashTerminalTitle is a no-op without ctx.ui.setTitle and after suppress', () => {
  assert.doesNotThrow(() => flashTerminalTitle(undefined, 'no ui'));
  assert.doesNotThrow(() => flashTerminalTitle({ hasUI: true, ui: {} } as unknown as PiContext, 'no setTitle'));
  assert.equal(isTitleFlashPendingForTests(), false);

  const { ctx, titles } = makeTitleCtx();
  suppressDesktopNotifications();
  flashTerminalTitle(ctx, 'suppressed', 'Octocode', 10);
  assert.deepEqual(titles, []);
  assert.equal(isTitleFlashPendingForTests(), false);
});

test('suppressDesktopNotifications restores a mid-flash title immediately, then goes quiet', async () => {
  const { ctx, titles } = makeTitleCtx();
  flashTerminalTitle(ctx, 'flash', 'Octocode', 10_000);
  assert.equal(isTitleFlashPendingForTests(), true);
  suppressDesktopNotifications();
  assert.equal(isTitleFlashPendingForTests(), false);
  // The "⚠" marker must not be left stuck in the terminal tab: suppress runs
  // the restore synchronously, and the cancelled timer never fires again.
  assert.deepEqual(titles, ['⚠ flash', 'Octocode']);
  await new Promise((r) => setTimeout(r, 30));
  assert.equal(titles.length, 2, 'no late restore into the next session');
});

test('replacement shutdown cancels a title restore without touching the stale session UI', async () => {
  const { ctx, titles } = makeTitleCtx();
  flashTerminalTitle(ctx, 'flash', 'Octocode', 10_000);
  suppressDesktopNotifications({ restoreTitle: false });
  assert.equal(isTitleFlashPendingForTests(), false);
  assert.deepEqual(titles, ['⚠ flash'], 'replacement session owns the next title paint');
  await new Promise((r) => setTimeout(r, 30));
  assert.equal(titles.length, 1, 'cancelled restore never calls the old session UI');
});
