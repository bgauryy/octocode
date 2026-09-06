import { sanitizeLine } from '../tui/palette.js';
/**
 * desktop-notify — dependency-free terminal "desktop" notifications.
 *
 * Two channels, both sanctioned by the verified pi API contract (§7 —
 * notifications/raw writes):
 *  - OSC 9 raw stdout writes (`\x1b]9;msg\x07`). pi-tui frames are single-buffer
 *    synchronized-output writes and Node serializes stdout writes, so zero-width
 *    out-of-band OSC sequences land safely BETWEEN TUI frames. Visible chars or
 *    cursor moves would desync — hence the strict sanitization below.
 *  - Terminal-title flash via `ctx.ui.setTitle` (pi's extension-level wrapper
 *    over OSC 0), restored by a single unref'd module-level timer.
 *
 * There is no built-in pi desktop-notification API; this mirrors pi's own
 * examples/extensions/notify.ts pattern.
 */

import type { PiContext } from '../types.js';


/** Hard cap for the OSC 9 payload — long messages get ellipsized to this many chars. */
export const OSC9_MAX_CHARS = 120;
export const TITLE_FLASH_MS = 4000;
const DEFAULT_RESTORE_TITLE = 'Octocode';

/**
 * Shutdown suppress flag. Set from session_shutdown BEFORE spawned workers are
 * torn down, so the burst of late killed/exit callbacks cannot emit desktop
 * notifications or title flashes into a dying (or brand new) session. Every
 * emitter in this module no-ops while set.
 */
let suppressed = false;

/** Suppress all desktop-notification emitters (session_shutdown ordering race guard). */
export function suppressDesktopNotifications(options: { restoreTitle?: boolean } = {}): void {
  suppressed = true;
  // A pending title restore must not fire into the next session. Restore it
  // immediately only while the shutdown context is still valid (normal quit);
  // Pi invalidates that context before replacement shutdown hooks run.
  if (options.restoreTitle !== false) restorePendingTitleFlash();
  clearTitleFlashTimer();
}

/** Whether the shutdown suppress flag is set (checked by callers that gate ledger events). */
export function desktopNotificationsSuppressed(): boolean {
  return suppressed;
}

/**
 * Lift the shutdown suppress flag so the next session's workers can notify
 * again. The suppress half runs on session_shutdown; this resume half must run
 * on session_start, mirroring resumeStatusPanel/resumeAwarenessPanel — otherwise
 * a single /new or /resume kills desktop notifications for the rest of the process.
 */
export function resumeDesktopNotifications(): void {
  suppressed = false;
}

/** Test hook: clear the shutdown suppress flag (mirrors setAgentProcessFactoryForTests resets). */
export function resumeDesktopNotificationsForTests(): void {
  resumeDesktopNotifications();
}

/**
 * Whether terminal notifications should be emitted at all.
 * Rule (documented order):
 *  1. `OCTOCODE_NOTIFY` truthy (anything but ''/'0'/'false') → always ON, even
 *     without a TTY — an explicit opt-in wins.
 *  2. `OCTOCODE_NOTIFY` set to ''/'0'/'false' → always OFF (explicit opt-out).
 *  3. Unset → ON only when stdout is a TTY, mirroring `colorEnabled`
 *     (src/tui/palette.ts): raw escape sequences must never leak into
 *     piped/redirected output.
 */
export function notificationsEnabled(env: NodeJS.ProcessEnv = process.env): boolean {
  const flag = env['OCTOCODE_NOTIFY'];
  if (flag !== undefined) {
    return !(flag === '' || flag === '0' || flag.toLowerCase() === 'false');
  }
  return process.stdout?.isTTY === true;
}

/**
 * Emit an OSC 9 notification (`ESC ] 9 ; message BEL`) — supported by iTerm2,
 * kitty, WezTerm, ghostty and others as a desktop notification.
 *
 * The payload is sanitized so the sequence can never desync the TUI: control
 * chars (including BEL, which would terminate the sequence early) become
 * spaces via `sanitizeLine`, embedded ESC bytes are stripped, and the payload
 * is hard-capped at {@link OSC9_MAX_CHARS} chars. Empty messages emit nothing.
 * No-op after {@link suppressDesktopNotifications}.
 */
export function emitOsc9(
  message: string,
  write: (s: string) => void = (s) => { process.stdout.write(s); },
): void {
  if (suppressed) return;
  // sanitizeLine handles C0/C1 controls (incl. BEL/newlines) but deliberately
  // leaves ESC (0x1B) alone for ANSI-aware rendering — inside an OSC payload an
  // ESC could start ST (`ESC \`) and truncate the notification, so strip it too.
  let text = sanitizeLine(String(message ?? '')).replace(/\x1b/g, ' ').trim();
  if (!text) return;
  if (text.length > OSC9_MAX_CHARS) text = `${text.slice(0, OSC9_MAX_CHARS - 1)}…`;
  write(`\x1b]9;${text}\x07`);
}

/** Single module-level restore timer — a newer flash supersedes any pending restore. */
let titleFlashTimer: ReturnType<typeof setTimeout> | undefined;
/** Restore action for the currently-displayed flash (run early on suppress). */
let pendingTitleRestore: (() => void) | undefined;

/**
 * Last title the harness set via ctx.ui.setTitle (recorded by applyOctocodeUi).
 * Flashes restore to THIS by default so a flash never clobbers the live
 * "Octocode · <session>" title with the bare brand constant.
 */
let lastSessionTitle = DEFAULT_RESTORE_TITLE;

/** Record the harness-owned terminal title so title flashes restore to it. */
export function recordSessionTitle(title: string): void {
  if (title) lastSessionTitle = title;
}

/**
 * Flash the terminal title with a warning marker, then restore it after
 * `timerMs`. Uses `ctx.ui.setTitle` (never raw writes), a single unref'd
 * timeout (never keeps the process alive), and is a no-op without a UI or
 * after {@link suppressDesktopNotifications}. Restores to `restoreTitle` when
 * given, else to the last {@link recordSessionTitle} value.
 */
export function flashTerminalTitle(
  ctx: PiContext | undefined,
  text: string,
  restoreTitle?: string,
  timerMs: number = TITLE_FLASH_MS,
): void {
  if (suppressed) return;
  if (typeof ctx?.ui?.setTitle !== 'function') return;
  clearTitleFlashTimer();
  const restore = restoreTitle ?? lastSessionTitle;
  ctx.ui?.setTitle?.(`⚠ ${text}`);
  pendingTitleRestore = (): void => { ctx.ui?.setTitle?.(restore); };
  const timer = setTimeout(() => {
    titleFlashTimer = undefined;
    pendingTitleRestore = undefined;
    ctx.ui?.setTitle?.(restore);
  }, timerMs);
  // Node timers expose unref(); test fake timers may not — optional-call it.
  (timer as { unref?: () => void }).unref?.();
  titleFlashTimer = timer;
}

/** Run the pending flash's restore immediately (shutdown path). */
function restorePendingTitleFlash(): void {
  if (titleFlashTimer !== undefined) {
    pendingTitleRestore?.();
  }
  pendingTitleRestore = undefined;
}

/** Cancel a pending title restore (called from session_shutdown so no timer outlives the session). */
export function clearTitleFlashTimer(): void {
  if (titleFlashTimer !== undefined) {
    clearTimeout(titleFlashTimer);
    titleFlashTimer = undefined;
  }
}

/** Test hook: whether a title-restore timer is currently pending. */
export function isTitleFlashPendingForTests(): boolean {
  return titleFlashTimer !== undefined;
}
