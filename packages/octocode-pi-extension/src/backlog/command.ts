import { sanitizeTerminalText } from '../shared/sanitize.js';
import fs from 'node:fs';
import path from 'node:path';
import type { ExtensionAPI, ExtensionCommandContext } from '@earendil-works/pi-coding-agent';
import { atomicWriteFileSync } from '../shared/atomic.js';
import { errorMessage } from '../shared/util.js';
import { say, wordCompletions, type Subcommand } from '../shared/commands.js';
import { workspacePaths } from '../shared/home.js';
import { openBacklog, type Backlog, type BacklogOptions } from './context.js';
import { showBoard } from './board.js';
import { boardText, exportMarkdown, isState, itemDetail, itemLine } from './format.js';
import { PRIORITIES, STATES, type Item } from './store.js';

const BACKLOG_USAGE = 'backlog [add <title> | <id> [backlog|todo|ongoing|done|delete] | do <id> | export] — the repository task board';

const ACTIONS = {
  do: 'Do it now',
  delegate: 'Delegate to a subagent',
  move: 'Move to…',
  edit: 'Edit',
  priority: 'Priority…',
  notes: 'Show notes',
  remove: 'Delete',
} as const;

/** The prompt that hands an item to the agent ("Do it now"). */
export function doItPrompt(item: Item): string {
  return `Work on backlog item ${item.ref}: ${sanitizeTerminalText(item.title)}${item.body ? `\n\n${sanitizeTerminalText(item.body)}` : ''}\n\nWhen finished, mark it done with the backlog tool (note what changed and how it was verified).`;
}

/** The prompt that asks the agent to hand an item to a subagent. */
function delegatePrompt(item: Item): string {
  return (
    `Delegate backlog item ${item.ref} to a subagent with the agent tool: ${sanitizeTerminalText(item.title)}${item.body ? `\n\n${sanitizeTerminalText(item.body)}` : ''}\n\n` +
    `Give the subagent the item id ${item.ref}; it records progress with the backlog tool and marks the item done (noting what changed and how it was verified) when finished.`
  );
}

/** Writes `<repo>/.octocode/backlog.md` and returns its path. */
function exportBacklog(backlog: Backlog, cwd: string, now = Date.now()): string {
  const { dir } = workspacePaths(cwd);
  fs.mkdirSync(dir, { recursive: true });
  const file = path.join(dir, 'backlog.md');
  atomicWriteFileSync(file, exportMarkdown(backlog.store, backlog.repoName, now));
  return file;
}

interface CommandDeps extends BacklogOptions {
  /** Repaint the footer counts after a change. */
  changed(ctx: ExtensionCommandContext): void;
}

/** The `/octocode backlog` subcommand (and its `/backlog` alias). */
export function backlogCommand(pi: ExtensionAPI, deps: CommandDeps): Subcommand {
  /** Runs one store change and repaints; returns the refusal text instead of throwing. */
  const apply = (ctx: ExtensionCommandContext, change: () => void): string | undefined => {
    try {
      change();
      deps.changed(ctx);
      return undefined;
    } catch (error) {
      return errorMessage(error);
    }
  };
  /** `apply`, telling the user the outcome (`done` on success, the refusal as a warning). */
  const applyAndSay = (ctx: ExtensionCommandContext, change: () => void, done?: string): void => {
    const refused = apply(ctx, change);
    if (refused) say(ctx, refused, 'warning');
    else if (done) say(ctx, done);
  };

  /** Mark `item` ongoing for this session and hand it to the agent, now or after the current run. */
  const handOff = (ctx: ExtensionCommandContext, backlog: Backlog, item: Item, prompt: (item: Item) => string): boolean => {
    let claimed: Item;
    try {
      claimed = backlog.store.update(item.ref, { state: 'ongoing' }, backlog.actor);
    } catch (error) {
      say(ctx, errorMessage(error), 'warning');
      return false;
    }
    deps.changed(ctx);
    pi.sendUserMessage(prompt(claimed), ctx.isIdle() ? undefined : { deliverAs: 'followUp' });
    if (!ctx.isIdle()) say(ctx, `${claimed.ref} is queued: the agent starts it after the current run.`);
    return true;
  };

  // The user's own move: may override another live session's hold (claiming it as ongoing is still refused).
  const move = (ctx: ExtensionCommandContext, backlog: Backlog, item: Item, state: (typeof STATES)[number]): string | undefined =>
    apply(ctx, () => backlog.store.update(item.ref, { state, version: item.version, force: true }, backlog.actor));

  /** Deletes `item` after the user confirms (`detail` is the dialog text). */
  const remove = async (ctx: ExtensionCommandContext, backlog: Backlog, item: Item, detail: string): Promise<void> => {
    if (!(await ctx.ui.confirm(`Delete ${item.ref}?`, detail))) return;
    applyAndSay(ctx, () => backlog.store.remove(item.ref, item.version), `Deleted ${item.ref}.`);
  };

  const add = async (ctx: ExtensionCommandContext, backlog: Backlog, given: string): Promise<void> => {
    const title = given || (ctx.hasUI ? ((await ctx.ui.input('New backlog item', 'title'))?.trim() ?? '') : '');
    if (!title) return say(ctx, 'Usage: /backlog add <title>', given || !ctx.hasUI ? 'warning' : 'info');
    let item: Item;
    try {
      item = backlog.store.add({ title, state: 'todo', createdBy: 'user', sourceSession: backlog.session });
    } catch (error) {
      return say(ctx, `Not added: ${errorMessage(error)}`, 'warning');
    }
    deps.changed(ctx);
    say(ctx, `Added ${itemLine(item, backlog.session)}`);
  };

  const edit = async (ctx: ExtensionCommandContext, backlog: Backlog, item: Item): Promise<void> => {
    const text = await ctx.ui.editor(`Edit ${item.ref} — first line is the title`, `${item.title}\n\n${item.body}`);
    if (text === undefined) return;
    const [first = '', ...rest] = text.split('\n');
    const title = first.trim();
    const body = rest.join('\n').trim();
    if (!title) return say(ctx, 'Not saved: the first line (the title) is empty.', 'warning');
    applyAndSay(ctx, () => backlog.store.update(item.ref, { title, body, version: item.version, force: true }, backlog.actor));
  };

  /** The action menu for one item; returns true when the item went to the agent (the board then closes). */
  const actions = async (ctx: ExtensionCommandContext, backlog: Backlog, ref: string): Promise<boolean> => {
    const item = backlog.store.get(ref);
    if (!item) {
      say(ctx, `No backlog item ${ref}.`, 'warning');
      return false;
    }
    const choice = await ctx.ui.select(itemLine(item, backlog.session), Object.values(ACTIONS));
    if (choice === ACTIONS.do) return handOff(ctx, backlog, item, doItPrompt);
    if (choice === ACTIONS.delegate) return handOff(ctx, backlog, item, delegatePrompt);
    if (choice === ACTIONS.move) {
      const state = await ctx.ui.select(`Move ${item.ref} to`, STATES.filter((state) => state !== item.state));
      const refused = state && isState(state) ? move(ctx, backlog, item, state) : undefined;
      if (refused) say(ctx, refused, 'warning');
    } else if (choice === ACTIONS.edit) await edit(ctx, backlog, item);
    else if (choice === ACTIONS.priority) {
      const priority = await ctx.ui.select(`Priority of ${item.ref}`, [...PRIORITIES]);
      const index = priority ? PRIORITIES.indexOf(priority as (typeof PRIORITIES)[number]) : -1;
      if (index >= 0) applyAndSay(ctx, () => backlog.store.update(item.ref, { priority: index, version: item.version, force: true }, backlog.actor));
    } else if (choice === ACTIONS.notes) {
      say(ctx, itemDetail(item, backlog.store.notes(item, 50), backlog.store.noteCount(item), Date.now(), { self: backlog.session, version: true }));
    } else if (choice === ACTIONS.remove) {
      await remove(ctx, backlog, item, `${sanitizeTerminalText(item.title)}\n\nThe item and its notes are removed for every session.`);
    }
    return false;
  };

  const doExport = (ctx: ExtensionCommandContext, backlog: Backlog) => {
    try {
      say(ctx, `Exported the backlog to ${exportBacklog(backlog, ctx.cwd)} (a snapshot; edits there are not read back).`);
    } catch (error) {
      say(ctx, `Export failed: ${errorMessage(error)}`, 'error');
    }
  };

  /** The TUI board; loops back to it after each action until the user closes it or hands an item to the agent. */
  const board = async (ctx: ExtensionCommandContext, backlog: Backlog): Promise<void> => {
    let selected: string | undefined;
    for (;;) {
      const choice = await showBoard(
        ctx,
        {
          title: `Backlog — ${backlog.repoName}`,
          self: backlog.session,
          load: () => backlog.store.list().items,
          move: (item, state) => move(ctx, backlog, item, state),
        },
        selected,
      );
      if (!choice) return;
      if (choice.action === 'add') await add(ctx, backlog, '');
      else if (choice.action === 'export') doExport(ctx, backlog);
      else {
        selected = choice.ref;
        const item = backlog.store.get(choice.ref);
        if (choice.action === 'do' && item) {
          if (handOff(ctx, backlog, item, doItPrompt)) return;
        } else if (await actions(ctx, backlog, choice.ref)) return;
      }
    }
  };

  /** Without a custom component (RPC): the board as a select list. */
  const selectBoard = async (ctx: ExtensionCommandContext, backlog: Backlog): Promise<void> => {
    const ADD = '+ Add an item';
    const EXPORT = 'Export to .octocode/backlog.md';
    for (;;) {
      const items = backlog.store.list().items;
      const lines = items.map((item) => itemLine(item, backlog.session));
      const choice = await ctx.ui.select(`Backlog — ${backlog.repoName}`, [ADD, ...lines, EXPORT]);
      if (choice === undefined) return;
      if (choice === ADD) await add(ctx, backlog, '');
      else if (choice === EXPORT) doExport(ctx, backlog);
      else {
        const item = items[lines.indexOf(choice)];
        if (item && (await actions(ctx, backlog, item.ref))) return;
      }
    }
  };

  return {
    description: BACKLOG_USAGE,
    complete: (prefix) => {
      const words = prefix.split(/\s+/);
      if (words.length <= 1) return wordCompletions([['add', 'add <title> — a new todo item'], ['do', 'do <id> — hand an item to the agent now'], ['export', 'write .octocode/backlog.md']], prefix);
      if (words.length === 2 && /^b?\d+$/i.test(words[0]!)) return wordCompletions([...STATES, 'delete'], words[1]!)?.map((item) => ({ ...item, value: `${words[0]} ${item.value}` })) ?? null;
      return null;
    },
    async handler(args, ctx) {
      let backlog: Backlog;
      try {
        backlog = openBacklog(ctx, deps);
      } catch (error) {
        return say(ctx, `Backlog unavailable: ${errorMessage(error)}`, 'error');
      }
      const line = args.trim();
      const [word = '', ...rest] = line.split(/\s+/);
      const tail = line.slice(word.length).trim();
      try {
        if (word === '') {
          if (ctx.hasUI && ctx.mode === 'tui') return await board(ctx, backlog);
          if (ctx.hasUI) return await selectBoard(ctx, backlog);
          return say(ctx, boardText(backlog.store, backlog.session));
        }
        if (word === 'add') return await add(ctx, backlog, tail);
        if (word === 'export') return doExport(ctx, backlog);
        if (word === 'do') {
          const item = rest[0] ? backlog.store.get(rest[0]) : undefined;
          if (!item) return say(ctx, rest[0] ? `No backlog item ${rest[0]}.` : 'Usage: /backlog do <id>', 'warning');
          handOff(ctx, backlog, item, doItPrompt);
          return;
        }
        const item = backlog.store.get(word);
        if (!item) return say(ctx, `Unknown backlog item or subcommand "${word}". Usage: /octocode ${BACKLOG_USAGE}`, 'warning');
        const verb = rest[0];
        if (verb === undefined) {
          if (ctx.hasUI) {
            await actions(ctx, backlog, item.ref);
            return;
          }
          return say(ctx, itemDetail(item, backlog.store.notes(item), backlog.store.noteCount(item), Date.now(), { self: backlog.session, version: true }));
        }
        if (isState(verb)) {
          const refused = move(ctx, backlog, item, verb);
          return refused ? say(ctx, refused, 'warning') : say(ctx, `Moved ${itemLine(backlog.store.get(item.ref)!, backlog.session)}`);
        }
        if (verb === 'delete') {
          if (!ctx.hasUI) return say(ctx, 'Deleting needs a confirmation: run it in the interactive UI.', 'warning');
          return await remove(ctx, backlog, item, sanitizeTerminalText(item.title));
        }
        return say(ctx, `Unknown action "${verb}". Usage: /octocode ${BACKLOG_USAGE}`, 'warning');
      } catch (error) {
        say(ctx, errorMessage(error), 'error');
      }
    },
  };
}
