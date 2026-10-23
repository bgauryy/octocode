import { StringEnum } from '@earendil-works/pi-ai';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';
import { clip, plural, resultBlock, toolHeader } from '../shared/render.js';
import { timingOf, timedTool } from '../shared/tool-timing.js';
import { storedText, visibleControls } from '../shared/sanitize.js';
import { contentText, textResult } from '../shared/util.js';
import { openBacklog, type Backlog, type BacklogOptions } from './context.js';
import { itemDetail, itemLine } from './format.js';
import { PRIORITIES, refuseSecrets, STATES, type Item, type State } from './item.js';

const DEFAULT_LIST_LIMIT = 20;
const DEFAULT_LIST_STATES: State[] = ['ongoing', 'todo'];
const DONE_NOTE = 'Marking an item done needs a note: say what changed and how its goal was verified.';

interface BacklogToolDetails {
  item?: Item;
  /** list: items shown and matched. */
  shown?: number;
  total?: number;
  /** remove: the removed item's ref. */
  removed?: string;
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

/** What the model reads about `backlog`: the description says what it is, `op` owns the operations, the guidelines only when to use it. */
function backlogPrompt(subagent: boolean): { description: string; ops: string; promptSnippet: string; promptGuidelines: string[] } {
  const description =
    'Track repository work across sessions: title, goal (done criteria), body. States: backlog proposed, todo accepted, ongoing claimed, done. Changes and times are logged. ' +
    (subagent ? 'A subagent proposes items and updates only items its parent has claimed. ' : 'An item another running session holds takes only notes. ') +
    'Text that looks like a secret is refused.';
  const ops =
    'list: items (default ongoing+todo; states/query/from filters); get: item details, times, origin, derived items, notes and changes; list/get: follow Next; ' +
    (subagent
      ? 'add: propose an item (state backlog); update: set state ongoing or done, or log progress with a note alone'
      : 'add: new item (default backlog); update: edit or claim with state ongoing; a note alone logs progress; remove: delete item and notes only for duplicates or user-requested removal');
  return {
    description,
    ops,
    promptSnippet: subagent ? 'Record progress on claimed items; propose follow-ups' : 'Propose, claim and close repository tasks',
    promptGuidelines: subagent
      ? ['Use backlog to note progress on the item your parent gave you and to propose follow-ups you find (from: that item, with a goal) instead of doing them; mark it done only after verifying its goal.']
      : [
          'Use backlog for work beyond this turn, not current-task steps. Propose follow-ups with a goal and from their source item; claim an item when starting it.',
          'Close only after verifying the goal.',
        ],
  };
}

/** The states a subagent may set: claiming and closing items its parent holds. */
const SUBAGENT_STATES = ['ongoing', 'done'] as const;

export function registerBacklogTool(pi: ExtensionAPI, options: BacklogOptions & { changed?: (ctx: ExtensionContext) => void }): void {
  const subagent = options.isSubagent;
  const { ops, ...prompt } = backlogPrompt(subagent);
  pi.registerTool(timedTool({
    name: 'backlog',
    label: 'Backlog',
    ...prompt,
    parameters: Type.Object({
      op: StringEnum(subagent ? ['list', 'get', 'add', 'update'] as const : ['list', 'get', 'add', 'update', 'remove'] as const, { description: ops }),
      id: Type.Optional(Type.String({ description: `${subagent ? 'get/update' : 'get/update/remove'}: item id, e.g. "B12"` })),
      title: Type.Optional(Type.String({ description: `${subagent ? 'add' : 'add/update'}: one-line summary` })),
      goal: Type.Optional(Type.String({ description: subagent ? 'add: Definition of Done, checkable' : 'add/update: Definition of Done, checkable; add needs it' })),
      paths: Type.Optional(Type.Array(Type.String(), { description: `files, dirs or components affected` })),
      why: Type.Optional(Type.String({ description: `why it matters` })),
      evidence: Type.Optional(Type.Array(Type.String(), { description: `observations: paths, measurements, links` })),
      plan: Type.Optional(Type.String({ description: `approach and steps` })),
      body: Type.Optional(Type.String({ description: `anything else; update replaces the whole text` })),
      state: Type.Optional(subagent ? StringEnum(SUBAGENT_STATES, { description: 'update: ongoing or done' }) : StringEnum(STATES, { description: 'add: initial state (default backlog); update: new state' })),
      priority: Type.Optional(StringEnum(PRIORITIES, { description: `${subagent ? 'add' : 'add/update'}: p0 urgent … p3 someday (default p2)` })),
      tags: Type.Optional(Type.Array(Type.String(), { description: `${subagent ? 'add' : 'add/update'}: short labels, e.g. ["docs"]` })),
      from: Type.Optional(Type.String({ description: `${subagent ? 'add' : 'add/update'}: the item this came from (split, follow-up, finding), e.g. "B12"${subagent ? '' : '; update "" clears'}; list: items derived from it` })),
      note: Type.Optional(Type.String({ description: `${subagent ? 'update' : 'update, or add with state ongoing/done'}: appended to the item log: why, or what you found` })),
      query: Type.Optional(Type.String({ description: 'list: text in any field, or a whole tag' })),
      states: Type.Optional(Type.Array(StringEnum(STATES), { description: 'list: states to show (default ongoing, todo)' })),
      limit: Type.Optional(Type.Integer({ description: `list/get: page size (items default ${DEFAULT_LIST_LIMIT}, notes default 10)` })),
      offset: Type.Optional(Type.Integer({ description: 'list/get: items or newest notes to skip; copy from Next (default 0)' })),
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
      const target = [args.op, args.id, args.state ? `→ ${args.state}` : '', args.from ? `from ${clip(String(args.from), 20)}` : '', args.title ? `"${clip(String(args.title), 80)}"` : '', args.query ? `"${clip(String(args.query), 80)}"` : ''].filter(Boolean).join(' ');
      return toolHeader(theme, context, 'Backlog', target);
    },
    renderResult(result, _options, theme, context) {
      const text = contentText(result.content);
      const details = (result.details ?? {}) as BacklogToolDetails;
      const summary = context.isError ? (text.split('\n', 1)[0] ?? '') : backlogSummary(context.args?.op, details, text);
      const firstIsSummary = context.isError || (context.args?.op !== 'list' && context.args?.op !== 'get');
      return resultBlock(theme, context, { summary, body: firstIsSummary ? text.split('\n').slice(1).join('\n') : text, ...timingOf(result.details) });
    },
  }, { exclusive: true }));

  type Params = { op: 'list' | 'get' | 'add' | 'update' | 'remove'; id?: string; title?: string; goal?: string; body?: string; paths?: string[]; why?: string; evidence?: string[]; plan?: string; state?: State; priority?: string; tags?: string[]; from?: string; note?: string; query?: string; states?: State[]; limit?: number; offset?: number };

  /** The descriptive fields a call sets, only those it names. */
  const text = ({ body, paths, why, evidence, plan }: Params) => ({
    ...(body !== undefined ? { body } : {}),
    ...(paths !== undefined ? { paths } : {}),
    ...(why !== undefined ? { why } : {}),
    ...(evidence !== undefined ? { evidence } : {}),
    ...(plan !== undefined ? { plan } : {}),
  });

  function run({ store, actor, session }: Backlog, params: Params) {
    const priority = params.priority === undefined ? undefined : PRIORITIES.indexOf(params.priority as (typeof PRIORITIES)[number]);
    if (params.op === 'list') {
      const states = params.states?.length ? params.states : DEFAULT_LIST_STATES;
      const limit = Math.max(1, Math.floor(params.limit ?? DEFAULT_LIST_LIMIT));
      const offset = Math.max(0, Math.floor(params.offset ?? 0));
      const filters = { ...(params.query ? { query: params.query } : {}), ...(params.from ? { from: params.from } : {}) };
      const { items, total } = store.list({ states, ...filters, limit, offset });
      const lines = items.map((item) => itemLine(item, actor.id, actor.owners));
      if (lines.length === 0) lines.push(`No ${states.join('/')} items${params.query ? ` matching "${visibleControls(params.query)}"` : ''}${params.from ? ` from ${visibleControls(params.from)}` : ''}.`);
      if (total > offset + items.length) lines.push(`Next: backlog ${JSON.stringify({ op: 'list', states, ...filters, limit, offset: offset + items.length })}`);
      const triage = states.includes('backlog') ? 0 : store.counts().backlog;
      if (triage > 0) lines.push(`${triage} proposed item(s) await triage (list with states: ["backlog"]).`);
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
        ...(params.goal !== undefined ? { goal: params.goal } : {}),
        ...text(params),
        state: wanted === 'ongoing' || wanted === 'done' ? 'todo' : wanted,
        ...(priority !== undefined ? { priority } : {}),
        ...(params.tags ? { tags: params.tags } : {}),
        ...(params.from ? { from: params.from } : {}),
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
      if (!item) throw new Error(`No backlog item ${visibleControls(params.id)}.`);
      const offset = Math.max(0, Math.floor(params.offset ?? 0));
      const limit = Math.max(1, Math.floor(params.limit ?? 10));
      const notes = store.notes(item, limit, offset);
      const total = store.noteCount(item);
      const origin = item.from === undefined ? undefined : store.get(item.from);
      const detail = itemDetail(item, notes, total, Date.now(), {
        self: actor.id,
        noteOffset: offset,
        ...(actor.owners ? { parents: actor.owners } : {}),
        ...(origin ? { origin } : {}),
        derived: store.derived(item),
      });
      const next = offset + notes.length < total ? `\nNext: backlog ${JSON.stringify({ op: 'get', id: item.ref, limit, offset: offset + notes.length })}` : '';
      return textResult(`${detail}${next}`, { item } as BacklogToolDetails);
    }
    if (params.op === 'remove') {
      if (subagent) throw new Error('A subagent cannot remove backlog items; report it to your parent.');
      const item = store.remove(params.id, undefined, actor);
      return textResult(`Removed ${item.ref} "${visibleControls(item.title)}" and its notes.`, { removed: item.ref } as BacklogToolDetails);
    }
    const fields = { title: params.title, goal: params.goal, ...text(params), priority, tags: params.tags, from: params.from };
    if (subagent && Object.values(fields).some((value) => value !== undefined)) throw new Error('A subagent changes only the state and notes of an item; report other changes to your parent.');
    // Moving an item back to todo or backlog would release the parent's claim behind its back.
    if (subagent && (params.state === 'todo' || params.state === 'backlog')) throw new Error(`A subagent sets only ongoing or done; ask your parent to move an item to ${params.state}.`);
    if (params.state === undefined && params.note === undefined && Object.values(fields).every((value) => value === undefined)) throw new Error('update needs something to change: state, note, title, goal, paths, why, evidence, plan, body, priority, tags or from.');
    // The goal is the done criterion an agent's items always have (`store.add`): an update may change it, not blank it.
    if (params.goal !== undefined && !storedText(params.goal)) throw new Error('update cannot clear the goal: give the new done criterion, a check someone else can run.');
    const current = store.get(params.id);
    if (params.state === 'done' && current?.state !== 'done' && !params.note?.trim()) throw new Error(DONE_NOTE);
    const item = store.update(
      params.id,
      {
        ...(params.title !== undefined ? { title: params.title } : {}),
        ...(params.goal !== undefined ? { goal: params.goal } : {}),
        ...text(params),
        ...(params.state !== undefined ? { state: params.state } : {}),
        ...(priority !== undefined ? { priority } : {}),
        ...(params.tags !== undefined ? { tags: params.tags } : {}),
        ...(params.from !== undefined ? { from: params.from.trim() === '' ? null : params.from } : {}),
        ...(params.note !== undefined ? { note: params.note } : {}),
      },
      actor,
    );
    return textResult(`Updated ${itemLine(item, actor.id, actor.owners)}`, { item } as BacklogToolDetails);
  }
}
