import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import path from 'node:path';
import { NOTIFY_ENV, NOTIFY_METHOD_ENV } from '../shared/env.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { clipText, contentText, firstLine } from '../shared/util.js';

/**
 * "Answer ready" and "waiting for you" signals for a terminal in the background, after Codex's
 * `tui.notification_method` / `notification_condition`: a desktop notification (OSC 9, OSC 777 or Kitty's OSC 99, else
 * the bell) and a `●` in the terminal title until the user is back.
 *
 * Pi enables focus reports (CSI ?1004) only in fullscreen mode, and that mode consumes them. So in the regular TUI this
 * enables them itself and reads `ESC [ I` / `ESC [ O`; without focus reports (fullscreen) it falls back to idleness: a
 * run that took long while the user typed nothing.
 */

export type NotifyCondition = 'off' | 'unfocused' | 'always';
export type NotifyMethod = 'osc9' | 'osc777' | 'osc99' | 'bel';
type Focus = 'unknown' | 'focused' | 'unfocused';

/** What fired a notice, in Claude Code's Notification vocabulary. */
export type NoticeType = 'idle_prompt' | 'permission_prompt' | 'elicitation_dialog';

export interface Notice {
  type: NoticeType;
  title: string;
  message: string;
}

const FOCUS_ON = '\x1b[?1004h';
const FOCUS_OFF = '\x1b[?1004l';
const FOCUS_IN = '\x1b[I';
const FOCUS_OUT = '\x1b[O';
const BEL = '\x07';
const MARK = '●';
/** Without focus reports: a run this long with no keypress counts as "the user stepped away". */
const AWAY_MS = 30_000;
/** No two notifications within this window (a prompt right after an answer, back-to-back dialogs). */
const DEBOUNCE_MS = 3_000;
/** An answer still unread after this long fires the `idle_prompt` Notification hook, as in Claude Code. */
const IDLE_PROMPT_MS = 60_000;
const BODY_MAX = 120;

export function notifyCondition(env: NodeJS.ProcessEnv = process.env): NotifyCondition {
  const value = env[NOTIFY_ENV]?.trim().toLowerCase();
  if (value === 'off' || value === '0' || value === 'false' || value === 'no') return 'off';
  if (value === 'always') return 'always';
  return 'unfocused';
}

/** The escape the terminal understands, from `OCTOCODE_NOTIFY_METHOD` or the terminal's own variables. */
export function notifyMethod(env: NodeJS.ProcessEnv = process.env): NotifyMethod {
  const chosen = env[NOTIFY_METHOD_ENV]?.trim().toLowerCase();
  if (chosen === 'osc9' || chosen === 'osc777' || chosen === 'osc99' || chosen === 'bel') return chosen;
  const program = `${env['TERM_PROGRAM'] ?? ''} ${env['LC_TERMINAL'] ?? ''}`.toLowerCase();
  if (env['KITTY_WINDOW_ID'] || env['TERM'] === 'xterm-kitty') return 'osc99';
  if (program.includes('iterm')) return 'osc9';
  if (program.includes('ghostty') || env['GHOSTTY_RESOURCES_DIR'] || program.includes('wezterm')) return 'osc777';
  return 'bel';
}

/** Text safe inside an OSC string: no control characters, no `;` (a field separator for OSC 777 / 99). */
function oscText(text: string): string {
  return clipText(firstLine(sanitizeTerminalText(text)).replace(/[\x00-\x1f\x7f-\x9f;]/g, ' ').trim(), BODY_MAX);
}

/** The bytes for one notification; inside tmux an OSC is wrapped in tmux's passthrough (the bell needs none). */
export function notificationSequence(method: NotifyMethod, title: string, body: string, env: NodeJS.ProcessEnv = process.env): string {
  const t = oscText(title);
  const b = oscText(body);
  let osc: string;
  switch (method) {
    case 'bel':
      return BEL;
    case 'osc9':
      osc = `\x1b]9;${t}: ${b}${BEL}`;
      break;
    case 'osc777':
      osc = `\x1b]777;notify;${t};${b}${BEL}`;
      break;
    case 'osc99':
      osc = `\x1b]99;i=octocode:d=0;${t}\x1b\\\x1b]99;i=octocode:p=body;${b}\x1b\\`;
      break;
  }
  return env['TMUX'] ? `\x1bPtmux;${osc.replaceAll('\x1b', '\x1b\x1b')}\x1b\\` : osc;
}

export interface NotifyOptions {
  /**
   * Notices for the Notification hooks, whether or not the terminal was signalled: a dialog when it opens, an unread
   * answer after a minute without input (Claude Code's `idle_prompt`).
   */
  onNotice?: (notice: Notice, ctx: ExtensionContext) => void;
  env?: NodeJS.ProcessEnv;
  write?: (data: string) => void;
}

/** Registers the notifier for the root session's TUI; returns nothing to drive: it follows Pi's events. */
export function registerNotify(pi: ExtensionAPI, options: NotifyOptions = {}): void {
  const env = options.env ?? process.env;
  const write = options.write ?? ((data: string) => void process.stdout.write(data));
  const condition = notifyCondition(env);
  let ctx: ExtensionContext | undefined;
  let focus: Focus = 'unknown';
  let lastInputAt = Date.now();
  let runStartedAt: number | undefined;
  let lastNoticeAt = 0;
  let marked = false;
  let unsubscribe: (() => void) | undefined;
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let lastAnswer = '';

  const baseTitle = (current: ExtensionContext): string => {
    const raw = pi.getSessionName();
    const name = raw ? sanitizeTerminalText(raw) : raw;
    const cwd = path.basename(current.cwd);
    return name ? `octocode · ${name} · ${cwd}` : `octocode · ${cwd}`;
  };
  const unmark = () => {
    if (!marked || !ctx) return;
    marked = false;
    ctx.ui.setTitle(baseTitle(ctx));
  };
  const clearIdle = () => {
    if (idleTimer) clearTimeout(idleTimer);
    idleTimer = undefined;
  };
  /** The user is (probably) not looking: unfocused, or no focus reports and a long run with no keypress. */
  const away = (now: number): boolean => {
    if (condition === 'always') return true;
    if (focus !== 'unknown') return focus === 'unfocused';
    return runStartedAt !== undefined && now - runStartedAt >= AWAY_MS && now - lastInputAt >= AWAY_MS;
  };
  const fire = (notice: Notice, hook: boolean) => {
    if (!ctx) return;
    if (hook) options.onNotice?.(notice, ctx);
    const now = Date.now();
    if (condition === 'off' || !away(now) || now - lastNoticeAt < DEBOUNCE_MS) return;
    lastNoticeAt = now;
    write(notificationSequence(notifyMethod(env), notice.title, notice.message, env));
    marked = true;
    ctx.ui.setTitle(`${MARK} ${baseTitle(ctx)}`);
  };

  pi.on('session_start', async (_event, current) => {
    unsubscribe?.();
    unsubscribe = undefined;
    clearIdle();
    ctx = current;
    marked = false;
    if (current.mode !== 'tui' || !current.hasUI) return;
    current.ui.setTitle(baseTitle(current));
    unsubscribe = current.ui.onTerminalInput((data) => {
      if (data === FOCUS_IN || data === FOCUS_OUT) {
        focus = data === FOCUS_IN ? 'focused' : 'unfocused';
        if (focus === 'focused') unmark();
        return { consume: true };
      }
      lastInputAt = Date.now();
      clearIdle();
      unmark();
      return undefined;
    });
    if (condition !== 'off') write(FOCUS_ON);
  });
  pi.on('session_info_changed', async (_event, current) => {
    if (current.mode === 'tui' && current.hasUI) current.ui.setTitle(`${marked ? `${MARK} ` : ''}${baseTitle(current)}`);
  });
  pi.on('session_shutdown', async () => {
    clearIdle();
    unsubscribe?.();
    unsubscribe = undefined;
    if (ctx?.mode === 'tui' && ctx.hasUI && condition !== 'off') write(FOCUS_OFF);
    unmark();
    ctx = undefined;
  });
  pi.on('agent_start', async () => {
    runStartedAt ??= Date.now();
    lastAnswer = '';
    clearIdle();
    unmark();
  });
  pi.on('agent_end', async (event) => {
    const last = [...event.messages].reverse().find((message) => message.role === 'assistant');
    // An interrupted run (Esc) means the user is right here; an error is worth a notice.
    if (!last || last.role !== 'assistant' || last.stopReason === 'aborted') return;
    lastAnswer = last.stopReason === 'error' ? `Failed: ${last.errorMessage ?? 'the run stopped with an error'}` : contentText(last.content) || 'Ready for input';
  });
  pi.on('agent_settled', async () => {
    const answer = lastAnswer;
    lastAnswer = '';
    if (ctx?.mode === 'tui' && answer) {
      fire({ type: 'idle_prompt', title: 'Octocode: answer ready', message: answer }, false);
      const current = ctx;
      clearIdle();
      idleTimer = setTimeout(() => options.onNotice?.({ type: 'idle_prompt', title: 'Octocode is waiting for your input', message: answer }, current), IDLE_PROMPT_MS);
      idleTimer.unref?.();
    }
    runStartedAt = undefined;
  });
  pi.on('ui_prompt_start', async (event) => {
    if (ctx?.mode !== 'tui') return;
    const type: NoticeType = event.kind === 'confirm' ? 'permission_prompt' : 'elicitation_dialog';
    fire({ type, title: 'Octocode: waiting for you', message: event.title ?? 'A question needs your answer' }, true);
  });
}
