import { renderDiff, type BashToolDetails, type Theme } from '@earendil-works/pi-coding-agent';
import { Box, type Component } from '@earendil-works/pi-tui';
import { formatDuration } from '../shared/format.js';
import { clip, expandKey, plural, resultBlock, resultText, timingOf, toolHeader, type RenderContext } from '../shared/render.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { contentText, isRecord } from '../shared/util.js';

/**
 * How the files tools draw: `Bash(cmd)` / `⎿  exit 0 · 12 lines` and the last lines, `File(edit a.ts)` /
 * `✓ edit a.ts +4 -1` with a short diff, and the background job report.
 */


/** What a bash result keeps: Pi's truncation fields, the background job, and the time it took. */
export interface BashDetails {
  truncation?: NonNullable<BashToolDetails>['truncation'];
  fullOutputPath?: string;
  job?: string;
  pid?: number;
  log?: string;
  durationMs?: number;
}

const BASH_STATUS = /^Command (?:exited with code (-?\d+)|timed out after (\d+) seconds|aborted|(terminated without an exit code))\.?$/;
const TRUNCATION_NOTICE = /^\[Showing .*Full output: \S+\]$/;

/**
 * The `⎿` summary and body of a bash result: `exit 0 · 12 lines`, `exit 1 · 3 lines` (drawn as an error),
 * `timed out after 2m`. Pi's status and truncation notices move into the summary; the body is the command output.
 */
function bashOutcome(text: string, isError: boolean, running = false): { summary: string; body: string; spill?: string } {
  let status: string | undefined;
  let total: number | undefined;
  let spill: string | undefined;
  const body: string[] = [];
  for (const line of sanitizeTerminalText(text).split('\n')) {
    const trimmed = line.trim();
    const match = BASH_STATUS.exec(trimmed);
    if (match) {
      status = match[1] !== undefined ? `exit ${match[1]}` : match[2] !== undefined ? `timed out after ${formatDuration(Number(match[2]) * 1000)}` : match[3] !== undefined ? 'terminated (no exit code)' : 'aborted';
      continue;
    }
    if (TRUNCATION_NOTICE.test(trimmed)) {
      const of = /\bof (\d+)\b/.exec(trimmed);
      if (of) total = Number(of[1]);
      spill = /Full output: (\S+)\]$/.exec(trimmed)?.[1];
      continue;
    }
    if (trimmed.startsWith('If it needs longer, rerun it with background: true')) continue;
    body.push(line);
  }
  while (body.length > 0 && body.at(-1)!.trim() === '') body.pop();
  while (body.length > 0 && body[0]!.trim() === '') body.shift();
  if (body.length === 1 && body[0] === '(no output)') body.length = 0;
  if (isError && !status) {
    // Not a command status (a missing directory, a refused command): the message is the summary.
    return { summary: body.shift() ?? 'failed', body: body.join('\n'), ...(spill ? { spill } : {}) };
  }
  const count = total ?? body.length;
  const size = count === 0 ? 'no output' : plural(count, 'line');
  return { summary: running ? `running · ${size}` : `${status ?? 'exit 0'} · ${size}`, body: body.join('\n'), ...(spill ? { spill } : {}) };
}

/** A result as a renderer sees it. */
interface ToolResultView {
  content: Array<{ type: string; text?: string }>;
  details?: unknown;
}

export function renderBashResult(result: ToolResultView, options: { expanded: boolean; isPartial: boolean }, theme: Theme, context: RenderContext & { isError: boolean }): Component {
  const details = (result.details ?? {}) as BashDetails;
  const text = contentText(result.content);
  const timing = timingOf(details);
  if (details.job && !context.isError) {
    return resultBlock(theme, context, { summary: `job ${details.job} started (pid ${details.pid ?? '?'})`, max: 0, expanded: options.expanded, ...(details.log ? { spill: details.log, spillLabel: 'log' } : {}), ...timing });
  }
  const outcome = bashOutcome(text, context.isError, options.isPartial);
  const spill = details.fullOutputPath ?? outcome.spill;
  return resultBlock(theme, context, { summary: outcome.summary, body: outcome.body, tail: true, expanded: options.expanded, ...(spill ? { spill } : {}), ...timing });
}

/** A finished background job: `● Job(bash-1: yarn build)` / `⎿  finished after 3s` and its output tail. */
export function renderJobMessage(text: string, expanded: boolean, theme: Theme, outputPad = 1): Component {
  const [head = '', ...rest] = text.split('\n');
  const ok = / finished after /.test(head);
  // Killed by the user or at shutdown: interrupted, not failed.
  const stopped = !ok && / stopped \(/.test(head);
  // "Background bash bash-1 finished after 3s: yarn build" → `● Job(bash-1: yarn build)` / `⎿ finished after 3s`.
  const parts = /^Background bash (\S+) (.+?): (.*)$/.exec(head);
  const context: RenderContext = { isPartial: false, isError: !ok, expanded, state: stopped ? { stopped: true } : {} };
  const log = rest[0]?.startsWith('Log: ') ? rest.shift() : undefined;
  // Pi draws custom messages flush left; pad them like a tool row.
  const box = new Box(outputPad, 0);
  box.addChild(toolHeader(theme, context, 'Job', parts ? `${parts[1]}: ${parts[3]}` : head));
  box.addChild(resultBlock(theme, context, { summary: parts?.[2] ?? head, body: rest.join('\n'), tail: true, error: !ok, stopped, ...(log ? { spill: log.slice(5), spillLabel: 'log' } : {}) }));
  return box;
}

export interface QueryOutcome {
  type: 'edit' | 'write' | 'delete';
  path: string;
  reasoning: string;
  ok: boolean;
  message: string;
  diff?: string;
}

/** Diff lines shown per file before expanding. */
const DIFF_PREVIEW_LINES = 6;

/** Size hint for a query in the call line: how many replacements, or how long the new content is. */
export function querySize(query: { type?: string; edits?: unknown[]; content?: string } | undefined): string {
  if (query?.type === 'edit' && Array.isArray(query.edits) && query.edits.length > 0) return ` · ${plural(query.edits.length, 'edit')}`;
  if (query?.type === 'write' && typeof query.content === 'string') {
    return ` · ${plural(query.content.split('\n').length, 'line')}`;
  }
  return '';
}

/** Added and removed line counts of a Pi diff (lines prefixed with + / -, not the file headers). */
export function diffStats(diff: string): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const line of diff.split('\n')) {
    if (/^\+(?!\+\+ )/.test(line)) added++;
    else if (/^-(?!-- )/.test(line)) removed++;
  }
  return { added, removed };
}

type FileArgs = { queries?: Array<{ type?: string; path?: string; reasoning?: string; edits?: unknown[]; content?: string }> };

export function renderFileCall(args: FileArgs, theme: Theme, context: RenderContext): Component {
  const queries = Array.isArray(args.queries) ? args.queries : [];
  const describe = (query: (typeof queries)[number] | undefined) => clip(`${query?.type ?? '…'} ${query?.path ?? ''}${querySize(query)}`, 200);
  const why = (query: (typeof queries)[number] | undefined) => (context.expanded && typeof query?.reasoning === 'string' && query.reasoning.trim() ? [theme.fg('dim', `  ↳ ${clip(query.reasoning, 200)}`)] : []);
  // One change: File(edit src/a.ts · 2 edits). Several: File(3 changes) with a sub-line each.
  if (queries.length <= 1) return toolHeader(theme, context, 'File', queries.length === 1 ? describe(queries[0]) : '', { lines: why(queries[0]) });
  const lines = queries.flatMap((query) => [`${theme.fg('accent', clip(query?.type ?? '…', 20))} ${theme.fg('mdCode', clip(query?.path ?? '', 200))}${theme.fg('dim', querySize(query))}`, ...why(query)]);
  return toolHeader(theme, context, 'File', `${queries.length} changes`, { lines });
}

export function renderFileResult(result: ToolResultView, { expanded }: { expanded: boolean }, theme: Theme, context: RenderContext): Component {
  const outcomes = (isRecord(result.details) && Array.isArray(result.details['outcomes']) ? result.details['outcomes'] : []) as QueryOutcome[];
  const timing = timingOf(result.details);
  if (outcomes.length === 0) {
    // Every change failed (or was rejected): the tool threw with the numbered outcome list.
    const [summary = '', ...rest] = resultText(result).split('\n').map((line) => line.replace(/^\d+\. FAILED /, ''));
    return resultBlock(theme, context, { summary, body: rest.join('\n'), ...timing });
  }
  const heads: string[] = [];
  const details: string[] = [];
  let diffLines = 0;
  for (const outcome of outcomes) {
    const stats = outcome.diff ? diffStats(outcome.diff) : undefined;
    const counts = stats ? ` ${theme.fg('toolDiffAdded', `+${stats.added}`)} ${theme.fg('toolDiffRemoved', `-${stats.removed}`)}` : '';
    const target = `${clip(outcome.type, 20)} ${theme.fg('mdCode', clip(outcome.path, 200))}`;
    const head = outcome.ok ? `${theme.fg('success', '✓')} ${target}${counts}` : `${theme.fg('error', '✗')} ${target}${theme.fg('error', `: ${clip(outcome.message, 300)}`)}`;
    heads.push(head);
    details.push(head);
    if (outcome.ok && outcome.diff) {
      const diff = renderDiff(sanitizeTerminalText(outcome.diff)).split('\n');
      diffLines += diff.length;
      details.push(...diff);
    }
  }
  const applied = outcomes.filter((outcome) => outcome.ok).length;
  const summary = outcomes.length === 1 ? '' : applied === outcomes.length ? `Applied ${plural(applied, 'change')}` : `Applied ${applied} of ${plural(outcomes.length, 'change')}`;
  if (expanded) return resultBlock(theme, context, { summary, lines: details, error: false, expanded, ...timing });
  if (outcomes.length === 1) {
    // One change: its outcome line, then the first diff lines, like Pi's edit tool.
    return resultBlock(theme, context, { summary: '', lines: details, max: DIFF_PREVIEW_LINES + 1, error: false, expanded, ...timing });
  }
  const hint = diffLines > 0 ? [theme.fg('dim', `… +${plural(diffLines, 'diff line')} (${expandKey()} to expand)`)] : [];
  return resultBlock(theme, context, { summary, lines: [...heads, ...hint], max: heads.length + hint.length, error: false, expanded, ...timing });
}
