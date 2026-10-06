import { StringEnum } from '@earendil-works/pi-ai';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';
import { clip, timingOf, plural, resultBlock, timedTool, toolHeader } from '../shared/render.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { textResult } from '../shared/util.js';
import { openBacklog, type Backlog, type BacklogOptions } from './context.js';
import { itemDetail, itemLine } from './format.js';
import { BODY_MAX, NOTE_MAX, PRIORITIES, refuseSecrets, STATES, TAGS_MAX, TITLE_MAX, type Item, type State } from './store.js';

const DEFAULT_LIST_LIMIT = 20;
const DEFAULT_LIST_STATES: State[] = ['ongoing', 'todo'];
const DONE_NOTE = 'Marking an item done needs a note: say what changed and how it was verified.';

interface BacklogToolDetails {
  item?: Item;
  /** list: items shown and matched. */
  shown?: number;
  total?: number;
  /** remove: the removed item's ref. */
  removed?: string;
  durationMs?: number;
}

/** The `⎿` line: `B12 → done`, `Added B3`, `Removed B1`, `3 items`. */
function backlogSummary(op: unknown, details: BacklogToolDetails, text: string): string {
  const item = details.item;
  if (op === 'list' && details.total !== undefined) return details.total === 0 ? 'No items' : details.total > (details.shown ?? 0) ? `${details.shown} of ${plural(details.total, 'item')}` : plural(details.total, 'item');
  if (op === 'remove' && details.removed) return `Removed ${details.removed}`;
  if (item && op === 'add') return `Added ${item.ref} · ${item.state}`;
  if (item && op === 'update') return `${item.ref} → ${item.state}`;
  if (item) return `${item.ref} · ${item.state} · ${item.title}`;
  return text.split('\n', 1)[0] ?? '';
}

/** What the model reads about `backlog`: the description owns ops and mechanics, the guidelines only when to use it. */
function backlogPrompt(subagent: boolean): { description: string; promptSnippet: string; promptGuidelines: string[] } {
  const description =
    'Track repository work across sessions. States: backlog (proposed) → todo (accepted) → ongoing (claimed) → done. ' +
    '`list`: items (default ongoing and todo; filter with `states`, `query`, `limit`). `get`: one item (`id`, e.g. "B12") with its notes. ' +
    (subagent
      ? '`add`: a new item (`title`, optional `body`, `priority`, `tags`). `update`: change an item (`id`); `note` alone appends progress. ' +
        'As a subagent: add proposes a backlog item; update sets ongoing or done, or appends a note, only on items your parent has claimed. '
      : '`add`: a new item (`title`, optional `body`, `priority`, `tags`); it lands in backlog unless `state` is given. ' +
        "`update`: change an item (`id`); `state: ongoing` claims it for you; `note` alone appends progress. `remove`: delete an item and its notes (`id`); only a duplicate or one the user asked to drop. An item another running session holds takes only notes. ") +
    'Text that looks like a secret is refused.';
  return {
    description,
    promptSnippet: subagent ? 'Record progress on backlog items your parent claimed and propose follow-ups' : 'Track repo tasks that outlive this turn: list, propose, claim and close them',
    promptGuidelines: subagent
      ? ['Use backlog to note progress on an item your parent gave you and to propose follow-ups you find instead of doing them; mark it done only after verifying the work, with a note saying what changed and how it was verified.']
      : [
          'Use backlog for work that outlives this turn. Propose discovered follow-ups in backlog; put accepted work in todo and keep routine steps in the current task.',
          'Set an item ongoing when you start it; mark it done only after verifying the work, with a note saying what changed and how it was verified.',
        ],
  };
}

/** The states a subagent may set: claiming and closing items its parent holds. */
const SUBAGENT_STATES = ['ongoing', 'done'] as const;

export function registerBacklogTool(pi: ExtensionAPI, options: BacklogOptions & { changed?: (ctx: ExtensionContext) => void }): void {
  const subagent = options.isSubagent;
  pi.registerTool(timedTool({
    name: 'backlog',
    label: 'Backlog',
    ...backlogPrompt(subagent),
    parameters: Type.Object({
      op: StringEnum(subagent ? ['list', 'get', 'add', 'update'] as const : ['list', 'get', 'add', 'update', 'remove'] as const, { description: 'Backlog operation' }),
      id: Type.Optional(Type.String({ description: `${subagent ? 'get/update' : 'get/update/remove'}: item id, e.g. "B12"` })),
      title: Type.Optional(Type.String({ maxLength: TITLE_MAX, description: `${subagent ? 'add' : 'add/update'}: one-line summary` })),
      body: Type.Optional(Type.String({ maxLength: BODY_MAX, description: `${subagent ? 'add' : 'add/update'}: details, acceptance criteria` })),
      state: Type.Optional(subagent ? StringEnum(SUBAGENT_STATES, { description: 'update: ongoing or done' }) : StringEnum(STATES, { description: 'add: initial state (default backlog); update: new state' })),
      priority: Type.Optional(StringEnum(PRIORITIES, { description: `${subagent ? 'add' : 'add/update'}: p0 urgent … p3 someday (default p2)` })),
      tags: Type.Optional(Type.Array(Type.String(), { maxItems: TAGS_MAX, description: `${subagent ? 'add' : 'add/update'}: short labels, e.g. ["docs"]` })),
      note: Type.Optional(Type.String({ maxLength: NOTE_MAX, description: subagent ? 'update: progress or done note appended to the item log' : 'update (or add with state ongoing/done): progress or done note appended to the item log' })),
      query: Type.Optional(Type.String({ description: 'list: text to find in titles, bodies and tags' })),
      states: Type.Optional(Type.Array(StringEnum(STATES), { description: 'list: states to show (default ongoing, todo)' })),
      limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 50, description: `list: most items (default ${DEFAULT_LIST_LIMIT})` })),
    }),
    async execute(_id, params, signal, _onUpdate, ctx) {
      signal?.throwIfAborted();
      const backlog = openBacklog(ctx, options);
      try {
        return run(backlog, params);
      } finally {
        options.changed?.(ctx);
      }
    },
    renderCall(args, theme, context) {
      const target = [args.op, args.id, args.state ? `→ ${args.state}` : '', args.title ? `"${clip(String(args.title), 80)}"` : '', args.query ? `"${clip(String(args.query), 80)}"` : ''].filter(Boolean).join(' ');
      return toolHeader(theme, context, 'Backlog', target);
    },
    renderResult(result, _options, theme, context) {
      const text = result.content.map((part) => (part.type === 'text' ? part.text : '')).join('\n');
      const details = (result.details ?? {}) as BacklogToolDetails;
      const summary = context.isError ? (text.split('\n', 1)[0] ?? '') : backlogSummary(context.args?.op, details, text);
      const firstIsSummary = context.isError || (context.args?.op !== 'list' && context.args?.op !== 'get');
      return resultBlock(theme, context, { summary, body: firstIsSummary ? text.split('\n').slice(1).join('\n') : text, ...timingOf(result.details) });
    },
  }, { exclusive: true }));

  type Params = { op: 'list' | 'get' | 'add' | 'update' | 'remove'; id?: string; title?: string; body?: string; state?: State; priority?: string; tags?: string[]; note?: string; query?: string; states?: State[]; limit?: number };

  function run({ store, actor, session }: Backlog, params: Params) {
    const priority = params.priority === undefined ? undefined : PRIORITIES.indexOf(params.priority as (typeof PRIORITIES)[number]);
    if (params.op === 'list') {
      const states = params.states?.length ? params.states : DEFAULT_LIST_STATES;
      const limit = params.limit ?? DEFAULT_LIST_LIMIT;
      const { items, total } = store.list({ states, ...(params.query ? { query: params.query } : {}), limit });
      const lines = items.map((item) => itemLine(item, actor.id, actor.owners));
      if (lines.length === 0) lines.push(`No ${states.join('/')} items${params.query ? ` matching "${sanitizeTerminalText(params.query)}"` : ''}.`);
      if (total > items.length) lines.push(`… ${total - items.length} more (raise limit or narrow with query).`);
      const triage = states.includes('backlog') ? 0 : store.counts().backlog;
      if (triage > 0) lines.push(`${triage} item(s) in backlog await triage (list with states: ["backlog"]).`);
      return textResult(lines.join('\n'), { shown: items.length, total } as BacklogToolDetails);
    }
    if (params.op === 'add') {
      if (!params.title?.trim()) throw new Error('add needs a title.');
      // A subagent proposes; the user or the parent accepts.
      const wanted = subagent ? 'backlog' : (params.state ?? 'backlog');
      // The note is stored by a second step: every refusal comes first, so a retry never leaves a duplicate behind.
      refuseSecrets({ note: params.note });
      if (params.note !== undefined && subagent) {
        // A proposal is not claimed by the parent, so a follow-up update with the note would be refused too.
        throw new Error('A subagent adds without a note: put the details in body.');
      }
      if (params.note !== undefined && wanted !== 'ongoing' && wanted !== 'done') {
        throw new Error('add takes a note only with state ongoing or done; put details in body, or add then update with the note.');
      }
      if (wanted === 'done' && !params.note?.trim()) throw new Error(DONE_NOTE);
      let item = store.add({
        title: params.title,
        ...(params.body !== undefined ? { body: params.body } : {}),
        state: wanted === 'ongoing' || wanted === 'done' ? 'todo' : wanted,
        ...(priority !== undefined ? { priority } : {}),
        ...(params.tags ? { tags: params.tags } : {}),
        createdBy: subagent ? `agent:${actor.id}` : 'agent',
        sourceSession: session,
      });
      if (wanted === 'ongoing' || wanted === 'done') {
        item = store.update(item.ref, { state: wanted, ...(params.note ? { note: params.note } : {}) }, actor);
      }
      return textResult(`Added ${itemLine(item, actor.id, actor.owners)}`, { item } as BacklogToolDetails);
    }
    if (!params.id?.trim()) throw new Error(`${params.op} needs an id, e.g. "B12".`);
    if (params.op === 'get') {
      const item = store.get(params.id);
      if (!item) throw new Error(`No backlog item ${sanitizeTerminalText(params.id)}.`);
      return textResult(itemDetail(item, store.notes(item), store.noteCount(item), Date.now(), { self: actor.id, ...(actor.owners ? { parents: actor.owners } : {}) }), { item } as BacklogToolDetails);
    }
    if (params.op === 'remove') {
      if (subagent) throw new Error('A subagent cannot remove backlog items; report it to your parent.');
      const item = store.remove(params.id, undefined, actor);
      return textResult(`Removed ${item.ref} "${sanitizeTerminalText(item.title)}" and its notes.`, { removed: item.ref } as BacklogToolDetails);
    }
    const fields = { title: params.title, body: params.body, priority, tags: params.tags };
    if (subagent && Object.values(fields).some((value) => value !== undefined)) throw new Error('A subagent changes only the state and notes of an item; report other changes to your parent.');
    // Moving an item back to todo or backlog would release the parent's claim behind its back.
    if (subagent && (params.state === 'todo' || params.state === 'backlog')) throw new Error(`A subagent sets only ongoing or done; ask your parent to move an item to ${params.state}.`);
    if (params.state === undefined && params.note === undefined && Object.values(fields).every((value) => value === undefined)) throw new Error('update needs something to change: state, note, title, body, priority or tags.');
    const current = store.get(params.id);
    if (params.state === 'done' && current?.state !== 'done' && !params.note?.trim()) throw new Error(DONE_NOTE);
    const item = store.update(
      params.id,
      {
        ...(params.title !== undefined ? { title: params.title } : {}),
        ...(params.body !== undefined ? { body: params.body } : {}),
        ...(params.state !== undefined ? { state: params.state } : {}),
        ...(priority !== undefined ? { priority } : {}),
        ...(params.tags !== undefined ? { tags: params.tags } : {}),
        ...(params.note !== undefined ? { note: params.note } : {}),
      },
      actor,
    );
    return textResult(`Updated ${itemLine(item, actor.id, actor.owners)}`, { item } as BacklogToolDetails);
  }
}
