import type { ExtensionCommandContext } from '@earendil-works/pi-coding-agent';
import { say, wordCompletions, type Subcommands } from '../shared/commands.js';
import { errorMessage } from '../shared/util.js';
import { lastInjection } from './inject.js';
import { memoryLabel } from './query.js';
import { MEMORY_KINDS, MemoryError, memoryLine, type Memory, type MemoryInput, type MemoryKind, type MemoryStore } from './store.js';
import { describeMemory } from './tool.js';

const LIST_LIMIT = 100;
const ADD = '+ Add a memory';

/** The editor template `add` and Edit open: header lines, a `---` line, then the body. */
export function memoryTemplate(memory?: Pick<Memory, 'title' | 'kind' | 'keywords' | 'scope' | 'body' | 'pinned'>): string {
  return [
    `title: ${memory?.title ?? ''}`,
    `kind: ${memory?.kind ?? 'fact'}  # ${MEMORY_KINDS.join(' | ')}`,
    `scope: ${memory?.scope ?? 'project'}  # project | global`,
    `keywords: ${memory?.keywords ?? ''}`,
    `pinned: ${memory?.pinned ? 'yes' : 'no'}`,
    '---',
    memory?.body ?? '',
  ].join('\n');
}

/** The fields of an edited template; unknown header lines are ignored, a bad kind or scope is an error. */
export function parseTemplate(text: string): MemoryInput {
  const lines = text.replace(/\r\n/g, '\n').split('\n');
  const split = lines.findIndex((line) => line.trim() === '---');
  const header = split === -1 ? lines : lines.slice(0, split);
  const body = split === -1 ? '' : lines.slice(split + 1).join('\n').trim();
  const input: MemoryInput = { body };
  for (const line of header) {
    const match = /^\s*(title|kind|scope|keywords|pinned)\s*:(.*)$/i.exec(line);
    if (!match) continue;
    const key = match[1]!.toLowerCase();
    const value = (key === 'title' ? match[2]! : match[2]!.replace(/#.*$/, '')).trim();
    if (key === 'title') input.title = value;
    else if (key === 'keywords') input.keywords = value;
    else if (key === 'pinned') input.pinned = /^(y|yes|true|1|on)$/i.test(value);
    else if (key === 'kind') {
      if (!(MEMORY_KINDS as readonly string[]).includes(value)) throw new MemoryError(`kind must be one of ${MEMORY_KINDS.join(', ')}.`);
      input.kind = value as MemoryKind;
    } else if (value !== 'project' && value !== 'global') throw new MemoryError('scope must be project or global.');
    else input.scope = value;
  }
  return input;
}

const section = (memory: Memory) => (memory.pinned ? 'Pinned' : memory.scope === 'project' ? 'Project' : 'Global');

/** One picker option: `Pinned · M3 (project, decision, pinned) Title — body…`. */
const optionOf = (memory: Memory) => `${section(memory)} · ${memoryLine(memory, 80)}`;

/** Pinned, then project, then global memories, newest first within each. */
function grouped(memories: readonly Memory[]): Memory[] {
  const order = { Pinned: 0, Project: 1, Global: 2 };
  return [...memories].sort((a, b) => order[section(a)] - order[section(b)] || b.updatedAt - a.updatedAt);
}

function textList(memories: readonly Memory[], title: string): string {
  if (memories.length === 0) return `${title}: none.`;
  const lines = [`${title}:`];
  let current = '';
  for (const memory of grouped(memories)) {
    if (section(memory) !== current) lines.push(`${(current = section(memory))}:`);
    lines.push(`  ${memoryLine(memory, 120)}`);
  }
  return lines.join('\n');
}

/**
 * `/octocode memory`: browse (pinned/project/global), `search <q>`, `add [title]`, `auto on|off`, `last`. The TUI picks
 * with `ui.select` and edits in `ui.editor`; without a UI every form prints text.
 */
export function registerMemoryCommand(commands: Subcommands, deps: { store: (ctx: ExtensionCommandContext) => MemoryStore; autoEnv: () => boolean }): void {
  commands.add('memory', {
    description: 'memory [search <q>|add|auto on|off|last] — browse and edit durable memories',
    complete: (prefix) => wordCompletions([['search ', 'find memories'], ['add', 'write a memory'], ['auto on', 'inject relevant memories'], ['auto off', 'stop injecting'], ['last', 'what was injected last']], prefix),
    handler: async (args, ctx) => {
      try {
        await run(args, ctx, deps.store(ctx), deps.autoEnv);
      } catch (error) {
        say(ctx, `memory: ${errorMessage(error)}`, error instanceof MemoryError ? 'warning' : 'error');
      }
    },
  });
}

async function run(args: string, ctx: ExtensionCommandContext, store: MemoryStore, autoEnv: () => boolean): Promise<void> {
  const [verb = '', ...restWords] = args.trim().split(/\s+/);
  const rest = restWords.join(' ').trim();
  if (verb === 'auto') {
    if (rest !== 'on' && rest !== 'off') return say(ctx, `Automatic memory injection is ${autoStatus(store, autoEnv)}. Usage: /memory auto on|off`, 'info');
    store.setAutoSetting(rest === 'on');
    return say(ctx, `Automatic memory injection ${rest === 'on' ? 'on' : 'off'} for every session.${rest === 'on' && !autoEnv() ? ' (Still off here: OCTOCODE_MEMORY_AUTO=0.)' : ''}`, 'info');
  }
  if (verb === 'last') {
    const last = lastInjection(ctx.sessionManager.getBranch());
    return say(ctx, last ?? 'No memories were injected on this branch yet.', 'info');
  }
  if (verb === 'add') return add(ctx, store, rest);
  if (verb === 'search') {
    if (!rest) return say(ctx, 'Usage: /memory search <words>', 'warning');
    return browse(ctx, store, store.search(rest, 'all', 20), `Memories matching "${rest}"`);
  }
  if (verb === '' || verb === 'list') return browse(ctx, store, store.list('all', LIST_LIMIT), `Memories (auto ${autoStatus(store, autoEnv)})`);
  return say(ctx, 'Usage: /memory [search <q>|add|auto on|off|last]', 'warning');
}

const autoStatus = (store: MemoryStore, autoEnv: () => boolean) => (!autoEnv() ? 'off (OCTOCODE_MEMORY_AUTO=0)' : store.autoSetting() ? 'on' : 'off');

async function add(ctx: ExtensionCommandContext, store: MemoryStore, text: string): Promise<void> {
  const sourceSession = ctx.sessionManager.getSessionId();
  if (!ctx.hasUI) {
    if (!text) return say(ctx, 'Usage: /memory add <title> [— body]', 'warning');
    const [title = '', ...body] = text.split(/\s+[—-]{1,2}\s+/);
    const { memory } = store.set({ title, body: body.join(' — '), author: 'user', sourceSession });
    return say(ctx, `Saved ${memoryLine(memory, 60)}`, 'info');
  }
  const edited = await ctx.ui.editor('New memory', memoryTemplate({ title: text, kind: 'fact', keywords: '', scope: 'project', body: '', pinned: false }));
  if (edited === undefined) return;
  const { memory } = store.set({ ...parseTemplate(edited), author: 'user', sourceSession });
  say(ctx, `Saved ${memoryLine(memory, 60)}`, 'info');
}

async function browse(ctx: ExtensionCommandContext, store: MemoryStore, memories: Memory[], title: string): Promise<void> {
  if (!ctx.hasUI) return say(ctx, textList(memories, title), 'info');
  let list = grouped(memories);
  for (;;) {
    const options = [...list.map(optionOf), ADD];
    const choice = await ctx.ui.select(title, options);
    if (choice === undefined) return;
    if (choice === ADD) return add(ctx, store, '');
    const picked = list[options.indexOf(choice)];
    if (!picked) return;
    const changed = await act(ctx, store, picked);
    if (changed === 'closed') return;
    // Show the list again with the change applied (a deleted memory drops out).
    list = grouped(list.flatMap((memory) => (memory.id !== picked.id ? [memory] : changed ? [changed] : [])));
  }
}

/** One memory's actions. Returns the updated memory, undefined when it was deleted, or 'closed' when dismissed. */
async function act(ctx: ExtensionCommandContext, store: MemoryStore, memory: Memory): Promise<Memory | undefined | 'closed'> {
  const label = memoryLabel(memory.id);
  const actions = ['Edit', memory.pinned ? 'Unpin' : 'Pin', memory.scope === 'project' ? 'Move to global' : 'Move to this project', 'Delete', 'Show'];
  const action = await ctx.ui.select(`${label} ${memory.title}`, actions);
  if (action === undefined) return memory;
  if (action === 'Show') {
    say(ctx, describeMemory(memory), 'info');
    return memory;
  }
  if (action === 'Delete') {
    if (!(await ctx.ui.confirm(`Delete ${label}?`, memory.title))) return memory;
    store.delete(memory.id);
    say(ctx, `Deleted ${label}.`, 'info');
    return undefined;
  }
  let input: MemoryInput;
  if (action === 'Edit') {
    const edited = await ctx.ui.editor(`Edit ${label}`, memoryTemplate(memory));
    if (edited === undefined) return memory;
    input = parseTemplate(edited);
  } else if (action === 'Pin' || action === 'Unpin') input = { pinned: action === 'Pin' };
  else input = { scope: memory.scope === 'project' ? 'global' : 'project' };
  const { memory: updated } = store.set({ ...input, id: memory.id, author: 'user' });
  say(ctx, `Updated ${memoryLine(updated, 60)}`, 'info');
  return updated;
}
