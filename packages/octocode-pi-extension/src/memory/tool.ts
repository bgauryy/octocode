import { StringEnum } from '@earendil-works/pi-ai';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';
import { clip, timingOf, plural, resultBlock, timedTool, toolHeader } from '../shared/render.js';
import { withDialog } from '../shared/locks.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { projectTrustNow } from '../shared/trust.js';
import { clipText, firstLine, textResult } from '../shared/util.js';
import { memoryLabel, parseMemoryId } from './query.js';
import { MEMORY_KINDS, MemoryError, memoryLine, refuseMemorySecrets, type Memory, type MemoryStore } from './store.js';

const DEFAULT_LIMIT = 10;

/** A note under reads in an untrusted project, where project memories stay hidden as they do from auto-injection. */
const HIDDEN_NOTE = '(untrusted project: project memories are hidden; only global ones are shown)';

/** What the model reads about `memory`: the description owns ops and mechanics, the guidelines only when to use it. */
function memoryPrompt(subagent: boolean): { description: string; promptSnippet: string; promptGuidelines: string[] } {
  const description =
    'Retrieve or maintain durable decisions and preferences across sessions. `search`: relevant notes by query; `get`: full note by id; `list`: pinned notes first, then newest. ' +
    (subagent
      ? 'Read-only in a subagent: report anything worth remembering to your parent. '
      : '`set`: add a memory (`title`, `body`, `keywords`, `kind`, `scope` default project, `pinned` = always injected), or update one with `id`; near-duplicates and secrets are refused. `delete`: remove `id`. ' +
        "Global or pinned changes need a trusted project and, with a UI, the user's confirmation. ") +
    'Read scopes default to all (global plus this repository); an untrusted project shows only global memories.';
  return {
    description,
    promptSnippet: subagent ? 'Search and read durable notes from earlier sessions' : 'Search, read or save durable notes shared across sessions',
    promptGuidelines: subagent
      ? ['Memory is read-only here: search it when earlier decisions may matter, and put anything worth remembering in your report.']
      : [
          'Save lasting user preferences or verified decisions that code and docs do not capture. Keep task progress in the task or backlog, and leave secrets out of memory.',
          'Search before saving and update a match instead of adding another; use scope global or pinned only when the user asks.',
        ],
  };
}

/** Every field the full (non-subagent) schema accepts. */
type MemoryParams = {
  op: 'search' | 'get' | 'list' | 'set' | 'delete';
  id?: string;
  query?: string;
  scope?: 'project' | 'global' | 'all';
  title?: string;
  body?: string;
  keywords?: string;
  kind?: (typeof MEMORY_KINDS)[number];
  pinned?: boolean;
  limit?: number;
};

/** The full view of one memory, for `get`. */
export function describeMemory(memory: Memory, now = Date.now()): string {
  const days = (at: number) => `${Math.max(0, Math.round((now - at) / 86_400_000))}d ago`;
  return [
    memoryLine(memory, 0),
    ...(memory.keywords ? [`keywords: ${memory.keywords}`] : []),
    `author: ${memory.author} · updated ${days(memory.updatedAt)} · used ${memory.useCount}×${memory.lastUsed ? ` (last ${days(memory.lastUsed)})` : ''}`,
    '',
    memory.body || '(no body)',
  ].join('\n');
}

function listText(memories: readonly Memory[], empty: string): string {
  return memories.length > 0 ? memories.map((memory) => memoryLine(memory)).join('\n') : empty;
}

/**
 * Registers the `memory` tool over the store `storeFor(ctx)` opens (one per repository). A subagent may only read: its
 * parent decides what is worth remembering. Global and pinned memories reach every session, so changing them needs a
 * trusted project (and the user's yes when there is a UI); an untrusted project reads only global memories, matching
 * what auto-injection shows there.
 */
export function registerMemoryTool(pi: ExtensionAPI, storeFor: (ctx: ExtensionContext) => MemoryStore, options: { isSubagent: boolean }): void {
  const ops = options.isSubagent ? ['search', 'get', 'list'] as const : ['search', 'get', 'list', 'set', 'delete'] as const;
  pi.registerTool(timedTool({
    name: 'memory',
    label: 'Memory',
    ...memoryPrompt(options.isSubagent),
    parameters: Type.Object({
      op: StringEnum(ops, { description: 'What to do' }),
      id: Type.Optional(Type.String({ description: options.isSubagent ? 'get: M<n>' : 'get/delete: M<n>; with set, updates that memory' })),
      query: Type.Optional(Type.String({ description: 'search: keywords' })),
      // A subagent only reads: the set-only fields would only cost context.
      ...(options.isSubagent
        ? { scope: Type.Optional(StringEnum(['project', 'global', 'all'] as const, { description: 'search/list: all (default)' })) }
        : {
            scope: Type.Optional(StringEnum(['project', 'global', 'all'] as const, { description: 'set: project (default) or global; search/list: all (default)' })),
            title: Type.Optional(Type.String({ maxLength: 120, description: 'set: specific one-line title' })),
            body: Type.Optional(Type.String({ maxLength: 1500, description: 'set: the note itself' })),
            keywords: Type.Optional(Type.String({ maxLength: 200, description: 'set: synonyms/terms that should retrieve this memory' })),
            kind: Type.Optional(StringEnum(MEMORY_KINDS, { description: 'set: what sort of note (default fact)' })),
            pinned: Type.Optional(Type.Boolean({ description: 'set: always inject this memory (keep pinned ones few and short)' })),
          }),
      limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 20, description: `search/list: most results (default ${DEFAULT_LIMIT})` })),
    }),
    async execute(_toolCallId, input, signal, _onUpdate, ctx) {
      // The subagent schema omits the set-only fields; set is refused there before they are read.
      const params = input as MemoryParams;
      signal?.throwIfAborted();
      const store = storeFor(ctx);
      const limit = params.limit ?? DEFAULT_LIMIT;
      const trusted = projectTrustNow(ctx) === true;
      const readScope = trusted ? (params.scope ?? 'all') : 'global';
      const note = (text: string) => (trusted ? text : `${text}\n${HIDDEN_NOTE}`);
      /** Refuses changing a memory that reaches every session (global or pinned) unless the project is trusted. */
      const guardWide = (wide: boolean, what: string) => {
        if (wide && !trusted) throw new MemoryError(`Refused: ${what} is shared with every session, and this is an untrusted project. Ask the user to /trust it, or save a plain project memory.`);
      };
      const id = params.id === undefined ? undefined : parseMemoryId(params.id);
      if (params.id !== undefined && id === undefined) throw new Error(`"${params.id}" is not a memory id (M<n>).`);
      const needId = (): number => {
        if (id === undefined) throw new Error(`${params.op} needs id.`);
        return id;
      };
      switch (params.op) {
        case 'search': {
          if (!params.query?.trim()) throw new Error('search needs query.');
          const hits = store.search(params.query, readScope, limit);
          return textResult(note(listText(hits, 'No memory matches.')), { ids: hits.map((hit) => hit.id) });
        }
        case 'get': {
          const found = store.get(needId());
          const memory = found && (trusted || found.scope === 'global') ? found : undefined;
          if (!memory) throw new MemoryError(`No memory M${id} here (it may belong to another repository).`);
          return textResult(describeMemory(memory), { ids: [memory.id] });
        }
        case 'list': {
          const memories = store.list(readScope, limit);
          const total = store.count(readScope);
          const more = total > memories.length ? `\n… ${total - memories.length} more; search, or raise limit.` : '';
          return textResult(note(`${listText(memories, 'No memories yet.')}${more}`), { ids: memories.map((memory) => memory.id), total });
        }
        case 'set':
        case 'delete': {
          if (options.isSubagent) throw new Error('Memory is read-only in a subagent: report it to your parent instead.');
          const found = id === undefined ? undefined : store.get(id);
          // Reads hide project memories in an untrusted project; changing one by id must not reveal or touch it either.
          if (found && !trusted && found.scope !== 'global') throw new MemoryError(`No memory M${id} here (it may belong to another repository).`);
          const current = found;
          if (params.op === 'delete') {
            if (current) guardWide(current.scope === 'global' || current.pinned, `deleting ${memoryLabel(current.id)}`);
            const removed = store.delete(needId());
            return textResult(`Deleted ${memoryLine(removed, 60)}`, { ids: [removed.id] });
          }
          if (params.scope === 'all') throw new Error('set scope is project or global.');
          const global = (params.scope ?? current?.scope) === 'global';
          const pinned = params.pinned ?? current?.pinned ?? false;
          guardWide(global || pinned || current?.scope === 'global', 'a global or pinned memory');
          // Refusals the store would raise come first, so the user is never asked about a save that then fails.
          refuseMemorySecrets(params);
          if (!current && !params.title?.trim()) throw new MemoryError('A new memory needs a title.');
          if ((global || pinned) && ctx.hasUI) {
            const label = [global ? 'global' : '', pinned ? 'pinned' : ''].filter(Boolean).join(', ');
            const title = firstLine(sanitizeTerminalText(params.title ?? current?.title ?? ''));
            const body = clipText(sanitizeTerminalText(params.body ?? current?.body ?? '').trim(), 400);
            const move = current && current.scope !== (params.scope ?? current.scope) ? `\nScope: ${current.scope} → ${params.scope}` : '';
            const text = `${title}${body ? `\n\n${body}` : ''}${move}\n\nIt will be shown to every ${global ? 'session in every repository' : 'session here'}.`;
            if (!(await withDialog(() => ctx.ui.confirm(`Save ${label} memory?`, text, signal ? { signal } : undefined), signal))) {
              throw new MemoryError('The user declined saving this global or pinned memory; save a plain project memory instead, or leave it.');
            }
          }
          const { memory, created } = store.set({
            ...(id !== undefined ? { id } : {}),
            ...(params.scope ? { scope: params.scope } : {}),
            ...(params.kind ? { kind: params.kind } : {}),
            ...(params.title !== undefined ? { title: params.title } : {}),
            ...(params.body !== undefined ? { body: params.body } : {}),
            ...(params.keywords !== undefined ? { keywords: params.keywords } : {}),
            ...(params.pinned !== undefined ? { pinned: params.pinned } : {}),
            author: 'agent',
            sourceSession: ctx.sessionManager.getSessionId(),
            ...(trusted ? {} : { hideProject: true }),
          });
          const hidden = !trusted && memory.scope === 'project' ? '\nThis untrusted project hides project memories from reads until the user /trusts it.' : '';
          return textResult(`${created ? 'Saved' : 'Updated'} ${memoryLine(memory, 60)}${hidden}`, { ids: [memory.id] });
        }
      }
      throw new Error(`Unknown op ${String(params.op)}.`);
    },
    renderCall(args, theme, context) {
      const detail = [args.id, args.query, (args as MemoryParams).title].find((value) => typeof value === 'string' && value.trim());
      const shown = detail === args.id || detail === undefined ? (detail ?? '') : `"${clip(String(detail), 100)}"`;
      return toolHeader(theme, context, 'Memory', [String(args.op ?? ''), shown].filter(Boolean).join(' '));
    },
    renderResult(result, _options, theme, context) {
      const text = result.content.map((part) => (part.type === 'text' ? part.text : '')).join('\n');
      const details = (result.details ?? {}) as { ids?: number[]; total?: number };
      const op = context.args?.op;
      const count = details.ids?.length ?? 0;
      // search and list count their rows; the others lead with what happened (`Saved M7 …`) or the error.
      if (!context.isError && op === 'search') return resultBlock(theme, context, { summary: count ? plural(count, 'hit') : 'No matches', body: count ? text : '', ...timingOf(result.details) });
      if (!context.isError && op === 'list') {
        const total = details.total ?? count;
        const summary = total === 0 ? 'No memories' : total > count ? `${count} of ${plural(total, 'memory', 'memories')}` : plural(total, 'memory', 'memories');
        return resultBlock(theme, context, { summary, body: count ? text : '', ...timingOf(result.details) });
      }
      const [head = '', ...rest] = text.split('\n');
      return resultBlock(theme, context, { summary: head, body: rest.join('\n'), ...timingOf(result.details) });
    },
  }, { exclusive: true }));
}
