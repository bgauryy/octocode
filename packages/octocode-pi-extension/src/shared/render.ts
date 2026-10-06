import { keyText, type ExtensionContext, type Theme, type ToolDefinition } from '@earendil-works/pi-coding-agent';
import type { TSchema } from 'typebox';
import { truncateToWidth, visibleWidth, wrapTextWithAnsi, type Component } from '@earendil-works/pi-tui';
import { formatDuration, shortPath } from './format.js';
import { sanitizeTerminalText } from './sanitize.js';
import { exclusive, takeTiming } from './locks.js';
import { clipText, contentText, firstLine, isRecord } from './util.js';

/**
 * Shared tool presentation, in the shape Claude Code and Codex use:
 *
 *   ● Name(summary) · 1.2s
 *     ⎿  one-line summary
 *        up to 3 preview lines
 *        … +K lines (ctrl+o to expand)
 *
 * The glyph is ○ before execution, ● accent while running (with a live 1s ticker), ● green when done and ✗ red on
 * error. Expanded shows every line. Every string drawn here is sanitized: tool arguments and output are untrusted.
 */

/** Result lines shown while collapsed. */
const COLLAPSED_LINES = 3;
/** Header summary budget in characters, before the width clip. */
const SUMMARY_CHARS = 120;
const SUMMARY_CHARS_EXPANDED = 200;
const GUTTER = '  ⎿  ';
const INDENT = '     ';

/**
 * The expand key. Pi loads extensions with their own copy of pi-tui, whose keybinding registry has no `app.*`
 * entries, so `keyText` comes back empty there; fall back to Pi's default binding.
 */
export function expandKey(): string {
  return appKey('app.tools.expand', 'ctrl+o');
}

/** The key bound to a Pi app action, or Pi's default binding when the registry has none (see `expandKey`). */
export function appKey(action: string, fallback: string): string {
  try {
    return keyText(action as never) || fallback;
  } catch {
    return fallback;
  }
}

/** `1 line`, `2 lines`; pass `many` for irregular plurals. */
export function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** One sanitized line of at most `max` characters, ending in `…` when cut. */
export function clip(text: string, max: number): string {
  return clipText(firstLine(sanitizeTerminalText(text).replace(/\t/g, ' ')).trim(), max);
}

/** Renderer state kept per tool row (`context.state`). */
export interface ToolRow {
  startedAt?: number;
  endedAt?: number;
  timer?: ReturnType<typeof setInterval>;
  /** From `details.durationMs`, for rows restored from a saved session. */
  durationMs?: number;
  /** The header drew a finished row without a duration; a later `resultBlock` redraws it once one is known. */
  missingDuration?: boolean;
  /** `checks 1.2s · queued 3s`: the time the call waited before it ran (see `timingOf`). */
  waits?: string;
  /** The call was interrupted (Esc, a kill, a requested stop), not failed: drawn `◼` in the warning colour. */
  stopped?: boolean;
}

/** The part of Pi's ToolRenderContext these helpers read. */
export interface RenderContext {
  lastComponent?: Component | undefined;
  state?: unknown;
  invalidate?: () => void;
  executionStarted?: boolean;
  isPartial?: boolean;
  isError?: boolean;
  expanded?: boolean;
  /** The call's arguments (Pi passes them to both renderers). */
  args?: unknown;
}

type Phase = 'pending' | 'running' | 'done' | 'error' | 'stopped';

function phaseOf(context: RenderContext): Phase {
  if (context.isPartial === false) return context.isError ? 'error' : 'done';
  return context.executionStarted ? 'running' : 'pending';
}

function rowState(context: RenderContext): ToolRow {
  return isRecord(context.state) ? (context.state as ToolRow) : {};
}

function glyph(theme: Theme, phase: Phase): string {
  switch (phase) {
    case 'pending':
      return theme.fg('dim', '○');
    case 'running':
      return theme.fg('accent', '●');
    case 'done':
      return theme.fg('success', '●');
    case 'error':
      return theme.fg('error', '✗');
    case 'stopped':
      return theme.fg('warning', '◼');
  }
}

/** Live row tickers, so a session switch can stop the ones whose rows will never get a final result. */
const rowTimers = new Set<ReturnType<typeof setInterval>>();

/** Stop every running row's ticker (on `session_shutdown`: the old session's rows are gone from the screen). */
export function stopRowTimers(): void {
  for (const timer of rowTimers) clearInterval(timer);
  rowTimers.clear();
}

/** Start the 1s ticker while running and stop it on the final result, following Pi's bash renderer. */
function track(context: RenderContext, phase: Phase): ToolRow {
  const state = rowState(context);
  if (phase === 'running') {
    state.startedAt ??= Date.now();
    if (!state.timer && context.invalidate) {
      const invalidate = context.invalidate;
      state.timer = setInterval(() => invalidate(), 1_000);
      state.timer.unref?.();
      rowTimers.add(state.timer);
    }
  } else if (phase === 'done' || phase === 'error') {
    if (state.timer) {
      clearInterval(state.timer);
      rowTimers.delete(state.timer);
    }
    state.timer = undefined;
    if (state.startedAt !== undefined) state.endedAt ??= Date.now();
  }
  return state;
}

/** Summaries of an interrupted call: Esc (`aborted`), a kill or a requested stop. */
const STOPPED = /^(?:error:\s*)?(?:(?:the )?(?:operation|request|run) (?:was )?aborted|aborted|interrupted|cancell?ed|stopped\b)/i;

/** A list of lines clipped to the render width (`wrap` lines wrap instead). */
class Lines implements Component {
  private rows: Array<{ text: string; wrap?: boolean; indent?: string }> = [];
  private header: { left: string; summary: string; right: string } | undefined;

  set(header: Lines['header'], rows: Lines['rows']): this {
    this.header = header;
    this.rows = rows;
    return this;
  }

  render(width: number): string[] {
    const out: string[] = [];
    if (this.header) {
      const { left, summary, right } = this.header;
      const room = width - visibleWidth(left) - visibleWidth(right);
      out.push(room >= 4 || !summary ? truncateToWidth(`${left}${summary ? truncateToWidth(summary, Math.max(0, room), '…') : ''}${right}`, width, '…') : truncateToWidth(`${left}${right}`, width, '…'));
    }
    for (const row of this.rows) {
      if (!row.wrap) {
        out.push(truncateToWidth(row.text, width, '…'));
        continue;
      }
      const indent = row.indent ?? '';
      const wrapped = wrapTextWithAnsi(row.text, Math.max(1, width - visibleWidth(indent)));
      for (const line of wrapped.length > 0 ? wrapped : ['']) out.push(`${indent}${line}`);
    }
    return out;
  }

  invalidate(): void {}
}

function lines(context: RenderContext): Lines {
  return context.lastComponent instanceof Lines ? context.lastComponent : new Lines();
}

/**
 * `<glyph> Name(summary) · 1.2s`, clipped to the width with the time kept. `lines` are extra sub-lines (one per
 * query of a batched call), clipped to the width; callers sanitize them (they may be styled). The duration comes from `durationMs`, else the live clock started
 * when execution began, else `details.durationMs` recorded by `resultBlock` (restored sessions).
 */
export function toolHeader(theme: Theme, context: RenderContext, name: string, summary: string, options: { durationMs?: number; meta?: string; lines?: string[] } = {}): Component {
  const base = phaseOf(context);
  const state = track(context, base);
  const phase: Phase = base === 'error' && state.stopped ? 'stopped' : base;
  const measured = state.startedAt !== undefined ? (state.endedAt ?? Date.now()) - state.startedAt : undefined;
  const duration = options.durationMs ?? measured ?? state.durationMs;
  const finished = phase === 'done' || phase === 'error' || phase === 'stopped';
  state.missingDuration = finished && duration === undefined;
  const meta = [options.meta ? clip(options.meta, 60) : '', duration !== undefined && phase !== 'pending' ? formatDuration(duration, true) : '', finished ? (state.waits ?? '') : ''].filter(Boolean);
  const text = clip(summary, context.expanded ? SUMMARY_CHARS_EXPANDED : SUMMARY_CHARS);
  const left = `${glyph(theme, phase)} ${theme.fg('toolTitle', theme.bold(clip(name, 60)))}${text ? theme.fg('muted', '(') : ''}`;
  const right = `${text ? theme.fg('muted', ')') : ''}${meta.length > 0 ? theme.fg('dim', ` · ${meta.join(' · ')}`) : ''}`;
  const rows = (options.lines ?? []).map((line) => ({ text: `  ${line}` }));
  return lines(context).set({ left, summary: text ? theme.fg('muted', text) : '', right }, rows);
}

function bodyLines(text: string): string[] {
  const rows = sanitizeTerminalText(text).replace(/\t/g, '   ').split('\n');
  while (rows.length > 0 && rows.at(-1)!.trim() === '') rows.pop();
  while (rows.length > 0 && rows[0]!.trim() === '') rows.shift();
  return rows;
}

interface ResultBlockOptions {
  /** The `⎿` line. Sanitized; only its first line is shown. */
  summary: string;
  /** Raw output: sanitized and coloured here. */
  body?: string;
  /** Pre-styled body lines (for diffs); the caller sanitized their text. Used instead of `body`. */
  lines?: string[];
  /** Pre-styled lines shown before the body only when expanded (a subagent's prompt); the caller sanitized them. */
  preface?: string[];
  /** What the collapsed hint calls the preface (`… +2 lines · prompt`); default `more`. */
  prefaceLabel?: string;
  /** Preview lines while collapsed (default 3). */
  max?: number;
  /** Preview the last lines instead of the first (command output). */
  tail?: boolean;
  /** Colour as an error; defaults to `context.isError`. */
  error?: boolean;
  /** Draw as interrupted rather than failed; defaults to an error whose summary says it was aborted or stopped. */
  stopped?: boolean;
  /** Where the full output was saved; shown short, and only when expanded. */
  spill?: string;
  /** The label before `spill` (default `saved`; `log` for a job's log). */
  spillLabel?: string;
  /** `details.durationMs`, so restored rows still show their time. */
  durationMs?: number;
  /** The pre-run waits `timingOf` read from the details; shown in the header. */
  waits?: string;
  /** Expanded view; defaults to `context.expanded` (pass renderResult's `options.expanded`). */
  expanded?: boolean;
}

/** `  ⎿  summary`, then a preview of the body and `… +K lines (ctrl+o to expand)`; the whole body when expanded. */
export function resultBlock(theme: Theme, context: RenderContext, options: ResultBlockOptions): Component {
  const state = rowState(context);
  const waitsChanged = options.waits !== undefined && state.waits !== options.waits;
  if (waitsChanged) state.waits = options.waits;
  if ((options.durationMs !== undefined && state.durationMs === undefined) || waitsChanged) {
    state.durationMs ??= options.durationMs;
    // The header already drew without this (a restored row, or waits known only now): redraw once, after this pass.
    if ((state.missingDuration || waitsChanged) && context.invalidate) {
      const invalidate = context.invalidate;
      queueMicrotask(() => invalidate());
    }
  }
  const summary = clip(options.summary, 400);
  const failed = options.error ?? context.isError === true;
  const stopped = options.stopped ?? (failed && STOPPED.test(summary));
  if (stopped && !state.stopped) {
    state.stopped = true;
    // The header may have drawn a failure glyph already: redraw it once as interrupted.
    if (context.invalidate) {
      const invalidate = context.invalidate;
      queueMicrotask(() => invalidate());
    }
  }
  const error = failed && !stopped;
  const color = stopped ? 'warning' : error ? 'error' : 'toolOutput';
  const rows: Array<{ text: string; wrap?: boolean; indent?: string }> = [];
  if (summary) rows.push({ text: `${theme.fg('dim', GUTTER)}${theme.fg(color, error && !/^(error|failed)\b|\bfailed\b/i.test(summary) ? `Error: ${summary}` : stopped ? summary.replace(/^error:\s*/i, '') : summary)}` });
  const body = options.lines ?? (options.body ? bodyLines(options.body).map((line) => theme.fg(color, line)) : []);
  const max = Math.max(0, options.max ?? COLLAPSED_LINES);
  const first = (index: number) => (!summary && index === 0 ? theme.fg('dim', GUTTER) : INDENT);
  const preface = options.preface ?? [];
  if (options.expanded ?? context.expanded) {
    preface.forEach((line, index) => rows.push({ text: line, wrap: true, indent: first(index) }));
    body.forEach((line, index) => rows.push({ text: line, wrap: true, indent: first(index + preface.length) }));
    if (options.spill) rows.push({ text: `${INDENT}${theme.fg('dim', `${options.spillLabel ?? 'saved'}: ${clip(shortPath(options.spill), 300)}`)}` });
  } else {
    // Blank lines waste preview rows; they still count as hidden and show when expanded.
    const filled = body.filter((line) => line.replace(/\u001b\[[0-9;]*m/g, '').trim() !== '');
    const shown = options.tail ? filled.slice(Math.max(0, filled.length - max)) : filled.slice(0, max);
    const hidden = body.length - shown.length;
    // The preface is named, not counted: `+2 lines` must mean two more lines of the body.
    const more = preface.length > 0 ? ` · ${options.prefaceLabel ?? 'more'}` : '';
    const folded = hidden > 0 || more !== '';
    // Summary-only tools (`⎿  Read 51 lines (ctrl+o to expand)`), or a saved path that only the expanded view shows.
    if (summary && (max === 0 ? folded || options.spill : !folded && options.spill)) rows[0] = { text: `${rows[0]!.text}${theme.fg('dim', `${max === 0 ? more : ''} (${expandKey()} to expand)`)}` };
    if (max === 0 && summary) return lines(context).set(undefined, rows);
    const hint = { text: `${INDENT}${theme.fg('dim', `${hidden > 0 ? `… +${plural(hidden, 'line')}${more}` : `… ${options.prefaceLabel ?? 'more'}`} (${expandKey()} to expand)`)}` };
    if (folded && options.tail) rows.push(hint);
    shown.forEach((line, index) => rows.push({ text: `${first(options.tail && folded ? 1 : index)}${line}` }));
    if (folded && !options.tail) rows.push(hint);
  }
  return lines(context).set(undefined, rows);
}

/** The file `capOutputToFile` (shared/spill.ts) saved the full output to, read back from its note. */
export function spillPath(text: string): string | undefined {
  return /Full output: (.+?); search it or read it by line range\.\]\s*$/.exec(text)?.[1];
}

/** The duration `timed` recorded in a result's details, if any. */
export function durationOf(details: unknown): number | undefined {
  return isRecord(details) && typeof details['durationMs'] === 'number' ? details['durationMs'] : undefined;
}

/** Waits shorter than this are not shown: they are routine. */
const WAIT_SHOW_MS = 250;

/**
 * `durationOf` plus the pre-run waits `timed` recorded (`checks 1.2s` in `tool_call` checks such as PreToolUse
 * hooks, `queued 3s` waiting for the rest of the batch's checks or the tool's lock), for `resultBlock`'s options.
 */
export function timingOf(details: unknown): { durationMs?: number; waits?: string } {
  const durationMs = durationOf(details);
  const wait = (key: string, label: string): string => {
    const value = isRecord(details) ? details[key] : undefined;
    return typeof value === 'number' && value >= WAIT_SHOW_MS ? `${label} ${formatDuration(value, true)}` : '';
  };
  const waits = [wait('checkMs', 'checks'), wait('queuedMs', 'queued')].filter(Boolean).join(' · ');
  return { ...(durationMs !== undefined ? { durationMs } : {}), ...(waits ? { waits } : {}) };
}

/**
 * Wrap a tool's `execute` so its result carries `details.durationMs` (details are kept in the session, so the time
 * survives a resume), plus `checkMs` / `queuedMs` when the call waited before it ran (see `takeTiming`). Existing
 * details are merged, never replaced. Pi passes the tool call id first; that is how the wait is found.
 */
export function timed<A extends unknown[], R>(execute: (...args: A) => Promise<R>): (...args: A) => Promise<R> {
  return (async (...args: A) => {
    const started = Date.now();
    const waits = typeof args[0] === 'string' ? takeTiming(args[0], started) : {};
    const result = await execute(...args);
    if (!isRecord(result)) return result;
    const timing = { durationMs: Date.now() - started, ...waits };
    const details = result['details'];
    if (details === undefined || details === null) return { ...result, details: timing };
    if (isRecord(details) && !Array.isArray(details)) return { ...result, details: { ...timing, ...details } };
    return result;
  }) as (...args: A) => Promise<R>;
}

/**
 * `pi.registerTool(timedTool({ ... }))`: the tool with its `execute` wrapped by `timed`. Prefer this to
 * `execute: timed(...)`, which loses the parameter types Pi's generic `registerTool` would infer. `exclusive` adds the
 * one-call-at-a-time lock of `exclusive()`; the recorded duration excludes the wait for it.
 */
export function timedTool<TParams extends TSchema, TDetails, TState>(
  tool: ToolDefinition<TParams, TDetails, TState>,
  options: { exclusive?: boolean } = {},
): ToolDefinition<TParams, TDetails, TState> {
  const execute = timed(tool.execute.bind(tool));
  return { ...tool, execute: options.exclusive ? exclusive(execute) : execute };
}

export function resultText(result: { content: Array<{ type: string; text?: string }> }): string {
  return sanitizeTerminalText(contentText(result.content, { image: '[image]' }));
}

/** A coloured footer status in the TUI; plain text for RPC clients, which would otherwise receive raw escape codes. */
export function statusColor(ctx: Pick<ExtensionContext, 'mode' | 'ui'>, color: Parameters<Theme['fg']>[0], text: string): string {
  return ctx.mode === 'tui' ? ctx.ui.theme.fg(color, text) : text;
}

/** Search terms first (what the call looks for), then where it looks. */
const TERM_KEYS = ['searchText', 'pattern', 'keywords', 'keywordsToSearch', 'symbolName', 'packageName', 'query', 'url', 'name'] as const;
const PATH_KEYS = ['path', 'filePath', 'uri'] as const;

function stringValue(value: unknown): string {
  return Array.isArray(value) ? value.filter((item) => typeof item === 'string').join(' ') : typeof value === 'string' ? value : '';
}

/**
 * The most telling value of one query: `term in path`, `owner/repo term`, `owner/repo/path` or the term alone.
 * Unsanitized: callers clip it.
 */
export function queryTarget(query: Record<string, unknown>): string {
  const repo = typeof query['owner'] === 'string' && typeof query['repo'] === 'string' ? `${query['owner']}/${query['repo']}` : undefined;
  const term = TERM_KEYS.map((key) => stringValue(query[key])).find(Boolean) ?? '';
  const path = PATH_KEYS.map((key) => stringValue(query[key])).find(Boolean) ?? '';
  const where = repo && path ? `${repo}/${path}` : (path || repo) ?? '';
  if (term && path) return `${term} in ${where}`;
  if (term) return repo ? `${repo} ${term}` : term;
  return where;
}
