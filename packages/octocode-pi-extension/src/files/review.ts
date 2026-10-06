import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { wordCompletions, type Subcommands } from '../shared/commands.js';
import { withDialog } from '../shared/locks.js';
import { statusColor } from '../shared/render.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { clipText } from '../shared/util.js';

/**
 * Optional human review of the `file` tool's batches. Off by default; `/octocode review on` (or OCTOCODE_REVIEW=1) asks before
 * every batch is applied, showing each change with the model's reasoning, and lets the user accept or reject changes one by one.
 */

export const REVIEW_ENV = 'OCTOCODE_REVIEW';
const STATUS_KEY = 'octocode-review';

export interface ReviewMode {
  on: boolean;
}

interface ChangeQuery {
  type: string;
  path: string;
  reasoning: string;
  edits?: Array<{ oldText: string; newText: string }> | undefined;
  content?: string | undefined;
}

// Model-supplied paths, reasoning and file text reach Pi's dialogs: strip terminal escapes first.
const clipLine = (line: string, max = 110) => clipText(sanitizeTerminalText(line), max);

function snippet(text: string, sign: '-' | '+', maxLines = 3): string[] {
  const lines = text.replace(/\n$/, '').split('\n');
  return [...lines.slice(0, maxLines).map((line) => `${sign} ${clipLine(line)}`), ...(lines.length > maxLines ? [`${sign} … ${lines.length - maxLines} more line(s)`] : [])];
}

/** What a change does, for the review dialog: what, why, and a bounded preview of the text involved. */
export function describeChange(query: ChangeQuery): string[] {
  const head = `${query.type} ${sanitizeTerminalText(query.path)}`;
  const why = `  why: ${sanitizeTerminalText(query.reasoning)}`;
  if (query.type === 'delete') return [head, why, '  removes the file'];
  if (query.type === 'write') return [head, why, ...snippet(query.content ?? '', '+', 5).map((line) => `  ${line}`)];
  const edits = query.edits ?? [];
  return [
    `${head} · ${edits.length} edit${edits.length === 1 ? '' : 's'}`,
    why,
    ...edits.slice(0, 3).flatMap((edit) => [...snippet(edit.oldText, '-', 2), ...snippet(edit.newText, '+', 2)].map((line) => `  ${line}`)),
    ...(edits.length > 3 ? [`  … ${edits.length - 3} more edit(s)`] : []),
  ];
}

const APPLY_ALL = 'Apply all';
const ONE_BY_ONE = 'Review one by one';
const REJECT_ALL = 'Reject all';

/**
 * Ask which changes of a batch to apply. Returns the indices the user rejected, or 'cancelled' when the dialog was
 * dismissed or interrupted (no verdict). Nothing is applied before this returns.
 */
export async function reviewChanges(ctx: ExtensionContext, queries: ChangeQuery[], signal?: AbortSignal): Promise<Set<number> | 'cancelled'> {
  return withDialog(() => askReview(ctx, queries, signal), signal);
}

async function askReview(ctx: ExtensionContext, queries: ChangeQuery[], signal?: AbortSignal): Promise<Set<number> | 'cancelled'> {
  const all = new Set(queries.keys());
  const options = signal ? { signal } : undefined;
  const list = queries.map((query, index) => sanitizeTerminalText(`${index + 1}. ${query.type} ${query.path} — ${query.reasoning}`)).join('\n');
  const choice = await ctx.ui.select(`Apply ${queries.length} file change${queries.length === 1 ? '' : 's'}?\n${list}`, queries.length === 1 ? [APPLY_ALL, REJECT_ALL] : [APPLY_ALL, ONE_BY_ONE, REJECT_ALL], options);
  if (choice === undefined || signal?.aborted) return 'cancelled';
  if (choice === APPLY_ALL) return new Set();
  if (choice !== ONE_BY_ONE) return all;
  const rejected = new Set<number>();
  for (const [index, query] of queries.entries()) {
    const accepted = await ctx.ui.confirm(`Change ${index + 1} of ${queries.length}`, describeChange(query).join('\n'), options);
    if (signal?.aborted) return 'cancelled';
    if (!accepted) rejected.add(index);
  }
  return rejected;
}

/** `/octocode review on|off`: the toggle and its footer note. */
export function registerReviewCommand(pi: ExtensionAPI, commands: Subcommands, mode: ReviewMode): void {
  const show = (ctx: ExtensionContext) => {
    if (ctx.hasUI) ctx.ui.setStatus(STATUS_KEY, mode.on ? statusColor(ctx, 'warning', 'review on') : undefined);
  };
  pi.on('session_start', async (_event, ctx) => show(ctx));
  commands.add('review', {
    description: 'review on|off — ask before each file batch',
    complete: (prefix) => wordCompletions(['on', 'off'], prefix),
    handler: async (args, ctx) => {
      const word = args.trim().toLowerCase();
      if (word === 'on' || word === 'off') mode.on = word === 'on';
      else if (word !== '') return ctx.ui.notify('Usage: /octocode review on | off', 'warning');
      show(ctx);
      ctx.ui.notify(mode.on ? 'Review on: every file change batch asks before it is applied.' : 'Review off: file changes apply directly.', 'info');
    },
  });
}
