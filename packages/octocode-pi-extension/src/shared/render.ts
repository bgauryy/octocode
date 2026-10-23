import { keyText, type ExtensionContext, type Theme } from '@earendil-works/pi-coding-agent';
import { truncateToWidth, visibleWidth, wrapTextWithAnsi, type Component } from '@earendil-works/pi-tui';
import { formatDuration, shortPath } from './format.js';
import { sanitizeTerminalText } from './sanitize.js';
import { phaseOf, rowState, runStart, track, type Phase, type RenderContext } from './tool-timing.js';
import { clipText, contentText, firstLine } from './util.js';

/**
 * Shared tool presentation, in the shape Claude Code and Codex use:
 *
 *   ● Name(summary) · 1.2s
 *     ⎿  one-line summary
 *        up to 3 preview lines
 *        … +K lines (ctrl+o to expand)
 *
 * The glyph is ○ before execution (`waiting` once Pi started the call but its checks or approval still run), ● accent
 * while running (the newest row with a live 1s clock), ● green when done and ✗ red on error. Expanded shows every line. Every string drawn here is sanitized: tool arguments and output are untrusted.
 */

/** Result lines shown while collapsed. */
export const COLLAPSED_LINES = 3;
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

const graphemes = new Intl.Segmenter();

/** One sanitized line of at most `max` terminal columns (a wide character counts 2), ending in `…` when cut: `clip` for lines that must not wrap. */
export function clipColumns(text: string, max: number): string {
  if (max <= 0) return '';
  const line = clip(text, Number.MAX_SAFE_INTEGER);
  if (visibleWidth(line) <= max) return line;
  let kept = '';
  let width = 0;
  for (const { segment } of graphemes.segment(line)) {
    const columns = visibleWidth(segment);
    if (width + columns > max - 1) break;
    kept += segment;
    width += columns;
  }
  return `${kept}…`;
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

/** Summaries of an interrupted call: Esc (`aborted`), a kill or a requested stop. */
const STOPPED = /^(?:error:\s*)?(?:(?:the )?(?:operation|request|run) (?:was )?aborted|aborted|interrupted|cancell?ed|stopped\b)/i;

/**
 * A list of lines clipped to the render width (`wrap` lines wrap instead). The output is cached per width until the
 * next `set`: pi-tui draws up to 60 frames a second while streaming, and its Box compares a child's lines by identity.
 */
class Lines implements Component {
  private rows: Array<{ text: string; wrap?: boolean; indent?: string }> = [];
  private header: { left: string; summary: string; right: string } | undefined;
  private cache: { width: number; lines: string[] } | undefined;

  set(header: Lines['header'], rows: Lines['rows']): this {
    this.header = header;
    this.rows = rows;
    this.cache = undefined;
    return this;
  }

  render(width: number): string[] {
    if (this.cache?.width === width) return this.cache.lines;
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
      // A width narrower than the indent still gets lines that fit: the indent and text are clipped together.
      for (const line of wrapped.length > 0 ? wrapped : ['']) out.push(truncateToWidth(`${indent}${line}`, width, '…'));
    }
    this.cache = { width, lines: out };
    return out;
  }

  invalidate(): void {
    this.cache = undefined;
  }
}

function lines(context: RenderContext): Lines {
  return context.lastComponent instanceof Lines ? context.lastComponent : new Lines();
}

/** Header sub-lines shown while collapsed, when more than one would be folded. */
const COLLAPSED_SUB_LINES = 3;

/**
 * `<glyph> Name(summary) · 1.2s`, clipped to the width with the time kept. `lines` are extra sub-lines (one per
 * query of a batched call), clipped to the width; collapsed, the first 3 show and `… +N more` counts the rest. Callers
 * sanitize them (they may be styled). Pi owns completed execution timing; while running, the newest row shows the time
 * since its run began (`timed` records it, so the tool must be wrapped by `timed`), and a row still waiting for its
 * checks, approval or lock says `waiting`. Custom messages can supply a duration.
 */
export function toolHeader(theme: Theme, context: RenderContext, name: string, summary: string, options: { durationMs?: number; meta?: string; lines?: string[] } = {}): Component {
  const base = phaseOf(context);
  const state = track(context, base);
  const start = base === 'running' ? runStart(context, state) : undefined;
  const waiting = base === 'running' && start === undefined;
  const phase: Phase = base === 'error' && state.stopped ? 'stopped' : waiting ? 'pending' : base;
  const measured = state.ticking && start !== undefined ? Date.now() - start : undefined;
  const duration = context.durationMs ?? options.durationMs ?? measured;
  const finished = phase === 'done' || phase === 'error' || phase === 'stopped';
  const time = waiting ? 'waiting' : duration !== undefined && phase !== 'pending' ? formatDuration(duration, true) : '';
  const meta = [options.meta ? clip(options.meta, 60) : '', time, finished ? (state.waits ?? '') : ''].filter(Boolean);
  const text = clip(summary, context.expanded ? SUMMARY_CHARS_EXPANDED : SUMMARY_CHARS);
  const left = `${glyph(theme, phase)} ${theme.fg('toolTitle', theme.bold(clip(name, 60)))}${text ? theme.fg('muted', '(') : ''}`;
  const right = `${text ? theme.fg('muted', ')') : ''}${meta.length > 0 ? theme.fg('dim', ` · ${meta.join(' · ')}`) : ''}`;
  const sub = options.lines ?? [];
  // Folding one line would cost the line it saves.
  const folded = !context.expanded && sub.length > COLLAPSED_SUB_LINES + 1;
  const rows = (folded ? sub.slice(0, COLLAPSED_SUB_LINES) : sub).map((line) => ({ text: `  ${line}` }));
  if (folded) rows.push({ text: `  ${theme.fg('dim', `… +${sub.length - COLLAPSED_SUB_LINES} more (${expandKey()} to expand)`)}` });
  return lines(context).set({ left, summary: text ? theme.fg('muted', text) : '', right }, rows);
}

/** `text` as display rows: terminal controls stripped, tabs widened, blank rows at both ends dropped. */
export function bodyLines(text: string): string[] {
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
  /** How many lines the whole body has when `body` holds only its tail (a collapsed command output), for `+K lines`. */
  totalLines?: number;
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
  /** The pre-run waits `timingOf` read from the details; shown in the header. */
  waits?: string;
  /** Expanded view; defaults to `context.expanded` (pass renderResult's `options.expanded`). */
  expanded?: boolean;
}

/** `  ⎿  summary`, then a preview of the body and `… +K lines (ctrl+o to expand)`; the whole body when expanded. */
export function resultBlock(theme: Theme, context: RenderContext, options: ResultBlockOptions): Component {
  const state = rowState(context);
  const waitsChanged = options.waits !== undefined && state.waits !== options.waits;
  if (waitsChanged) {
    state.waits = options.waits;
    // The header already drew without these waits: redraw once, after this pass.
    if (context.invalidate) {
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
  // Raw body lines are coloured only where shown: a collapsed preview colours its few lines, not the whole output.
  const raw = options.lines ? undefined : options.body ? bodyLines(options.body) : [];
  const paint = (line: string) => (raw ? theme.fg(color, line) : line);
  const body = options.lines ?? raw!;
  const max = Math.max(0, options.max ?? COLLAPSED_LINES);
  const first = (index: number) => (!summary && index === 0 ? theme.fg('dim', GUTTER) : INDENT);
  const preface = options.preface ?? [];
  if (options.expanded ?? context.expanded) {
    preface.forEach((line, index) => rows.push({ text: line, wrap: true, indent: first(index) }));
    body.forEach((line, index) => rows.push({ text: paint(line), wrap: true, indent: first(index + preface.length) }));
    if (options.spill) rows.push({ text: `${INDENT}${theme.fg('dim', `${options.spillLabel ?? 'saved'}: ${clip(shortPath(options.spill), 300)}`)}` });
  } else {
    // Blank lines waste preview rows; they still count as hidden and show when expanded.
    const filled = body.filter((line) => (raw ? line : line.replace(/\u001b\[[0-9;]*m/g, '')).trim() !== '');
    const shown = (options.tail ? filled.slice(Math.max(0, filled.length - max)) : filled.slice(0, max)).map(paint);
    const hidden = Math.max(options.totalLines ?? 0, body.length) - shown.length;
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

export function resultText(result: { content: Array<{ type: string; text?: string }> }): string {
  return sanitizeTerminalText(contentText(result.content, { image: '[image]' }));
}

type ColorToken = Parameters<Theme['fg']>[0];

/** What `statusColor` painted, by its output, so the footer can repaint it in the live theme (`livePaint`). */
const painted = new Map<string, { color: ColorToken; text: string }>();
/** Bounded: counts in statuses change; a status evicted while still shown keeps its colours until the next theme switch. */
const PAINTED_MAX = 64;

/**
 * A coloured footer status in the TUI; plain text for RPC clients, which would otherwise receive raw escape codes.
 * The footer repaints it with the theme of each frame, so a theme switch reaches statuses set before it.
 */
export function statusColor(ctx: Pick<ExtensionContext, 'mode' | 'ui'>, color: ColorToken, text: string): string {
  if (ctx.mode !== 'tui') return text;
  const out = ctx.ui.theme.fg(color, text);
  painted.delete(out);
  painted.set(out, { color, text });
  if (painted.size > PAINTED_MAX) painted.delete(painted.keys().next().value!);
  return out;
}

/** A status `statusColor` made, painted again with `theme` (the live one); any other text as it is. */
export function livePaint(text: string, theme: Pick<Theme, 'fg'>): string {
  const entry = painted.get(text);
  return entry ? theme.fg(entry.color, entry.text) : text;
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
