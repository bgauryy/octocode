import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import { kindOption } from '../permission/dialog.js';
import { foldLines } from '../permission/effect.js';
import { plural } from '../shared/render.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { clipText } from '../shared/util.js';

/**
 * The review of a `file` batch: the `file.workspace` permission dialog. In `manual` mode every batch shows each
 * change with the system's reason lines and the model's reasoning (labelled unverified, last), and the user applies all, rejects all, or decides change by change.
 */

interface ChangeQuery {
  type: string;
  path: string;
  reasoning: string;
  edits?: Array<{ oldText: string; newText: string }> | undefined;
  content?: string | undefined;
}

// Model-supplied paths, reasoning and file text reach Pi's dialogs: strip terminal escapes first.
const clipLine = (line: string, max = 110) => clipText(sanitizeTerminalText(line), max);

/** What the agent says a change is for, as the dialogs label it. */
const agentSays = (reasoning: string): string => `agent says (unverified): ${clipLine(foldLines(reasoning), 160)}`;

function snippet(text: string, sign: '-' | '+', maxLines = 3): string[] {
  const lines = text.replace(/\n$/, '').split('\n');
  return [...lines.slice(0, maxLines).map((line) => `${sign} ${clipLine(line)}`), ...(lines.length > maxLines ? [`${sign} … ${lines.length - maxLines} more line(s)`] : [])];
}

/** What a change does, for the review dialog: what, a bounded preview of the text involved, then what the agent says about it (unverified, last). */
export function describeChange(query: ChangeQuery): string[] {
  const head = `${query.type} ${sanitizeTerminalText(query.path)}`;
  const says = `  ${agentSays(query.reasoning)}`;
  if (query.type === 'delete') return [head, '  removes the file', says];
  if (query.type === 'write') return [head, ...snippet(query.content ?? '', '+', 5).map((line) => `  ${line}`), says];
  const edits = query.edits ?? [];
  return [
    `${head} · ${plural(edits.length, 'edit')}`,
    ...edits.slice(0, 3).flatMap((edit) => [...snippet(edit.oldText, '-', 2), ...snippet(edit.newText, '+', 2)].map((line) => `  ${line}`)),
    ...(edits.length > 3 ? [`  … ${edits.length - 3} more edit(s)`] : []),
    says,
  ];
}

const APPLY_ALL = 'Apply all';
/** Apply these and every later file change in the repository this session without reviewing. */
const APPLY_SESSION = kindOption('file.workspace')!;
const ONE_BY_ONE = 'Review one by one';
const REJECT_ALL = 'Reject all';
const REJECT_ALL_NOTE = 'Reject all and tell the agent why…';
const APPLY = 'Apply';
const REJECT = 'Reject';
const REJECT_NOTE = 'Reject and tell the agent why…';

/**
 * Ask which changes of a batch to apply. Maps rejected indices to optional user notes, or returns 'cancelled' when the dialog was
 * dismissed or interrupted (no verdict). Nothing is applied before this returns. `Permission.check` runs it, inside
 * its one-dialog-at-a-time lock.
 */
export async function askReview(ctx: Pick<ExtensionContext, 'ui'>, queries: ChangeQuery[], signal?: AbortSignal, reasonLines: readonly string[] = []): Promise<Map<number, string | undefined> | 'cancelled' | 'session'> {
  const all = (note?: string) => new Map(queries.map((_, index) => [index, note]));
  const options = signal ? { signal } : undefined;
  // What changes first, then the system's reason lines (the same card every prompt leads with), then what the agent says.
  const list = queries.map((query, index) => sanitizeTerminalText(`${index + 1}. ${query.type} ${query.path}`)).join('\n');
  const says = queries.map((query, index) => `  ${index + 1}. ${agentSays(query.reasoning)}`).join('\n');
  const choice = await ctx.ui.select(`Apply ${plural(queries.length, 'file change')}?\n${[list, ...reasonLines, says].join('\n')}`, [APPLY_ALL, APPLY_SESSION, ...(queries.length > 1 ? [ONE_BY_ONE] : []), REJECT_ALL, REJECT_ALL_NOTE], options);
  if (choice === undefined || signal?.aborted) return 'cancelled';
  if (choice === APPLY_ALL) return new Map();
  if (choice === APPLY_SESSION) return 'session';
  if (choice === REJECT_ALL) return all();
  if (choice === REJECT_ALL_NOTE) {
    const note = await ctx.ui.input('Tell the agent why', 'what to change instead', options);
    return note === undefined || signal?.aborted ? 'cancelled' : all(note.trim() || undefined);
  }
  if (choice !== ONE_BY_ONE) return 'cancelled';
  const rejected = new Map<number, string | undefined>();
  for (const [index, query] of queries.entries()) {
    const [head, ...details] = describeChange(query);
    const decision = await ctx.ui.select(`Change ${index + 1} of ${queries.length}\n${[head, ...reasonLines, ...details].join('\n')}`, [APPLY, REJECT, REJECT_NOTE], options);
    if (decision === undefined || signal?.aborted) return 'cancelled';
    if (decision === APPLY) continue;
    if (decision === REJECT) rejected.set(index, undefined);
    else if (decision === REJECT_NOTE) {
      const note = await ctx.ui.input('Tell the agent why', 'what to change instead', options);
      if (note === undefined || signal?.aborted) return 'cancelled';
      rejected.set(index, note.trim() || undefined);
    } else return 'cancelled';
  }
  return rejected;
}
