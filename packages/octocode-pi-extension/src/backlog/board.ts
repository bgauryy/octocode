import { sanitizeTerminalText } from '../shared/sanitize.js';
import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Key, matchesKey, truncateToWidth } from '@earendil-works/pi-tui';
import { priorityName, type Item as BacklogItem, type State as BacklogState } from './store.js';
import { BOARD_ORDER as ORDER, STATE_TITLE as HEADINGS } from './format.js';

/** What the user picked on the board; the command runs it and reopens the board unless it handed work to the agent. */
type BoardChoice = { action: 'open' | 'do'; ref: string } | { action: 'add' } | { action: 'export' };

interface BoardSource {
  title: string;
  /** Fresh items (all states), re-read after every change. */
  load(): BacklogItem[];
  /** Move an item one column; returns a refusal to show, or undefined. */
  move(item: BacklogItem, state: BacklogState): string | undefined;
  /** Who "you" is, to mark items this session holds. */
  self: string;
}

const DONE_SHOWN = 5;

/** Items in board order: grouped by state, Done cut to the most recent few. */
export function boardRows(items: BacklogItem[]): BacklogItem[] {
  return ORDER.flatMap((state) => {
    const group = items.filter((item) => item.state === state);
    return state === 'done' ? group.sort((a, b) => (b.doneAt ?? b.updatedAt) - (a.doneAt ?? a.updatedAt)).slice(0, DONE_SHOWN) : group;
  });
}

/**
 * The board: a section per state (backlog, todo, ongoing, done, top to bottom) with counts, ↑↓ select, ←→ move the
 * selected item to the section above/below,
 * Enter item actions, `a` add, `d` do it now, `e` export, Esc close.
 */
export function showBoard(ctx: ExtensionContext, source: BoardSource, selectRef?: string): Promise<BoardChoice | undefined> {
  return ctx.ui.custom<BoardChoice | undefined>((tui, theme, _keybindings, done) => {
    let items = source.load();
    let rows = boardRows(items);
    let cursor = Math.max(0, rows.findIndex((item) => item.ref === selectRef));
    let flash: string | undefined;
    let cache: string[] | undefined;
    const refresh = () => {
      cache = undefined;
      tui.requestRender();
    };
    const reload = (ref?: string) => {
      items = source.load();
      rows = boardRows(items);
      const at = ref ? rows.findIndex((item) => item.ref === ref) : -1;
      cursor = at >= 0 ? at : Math.min(cursor, Math.max(0, rows.length - 1));
    };

    function handleInput(data: string): void {
      const current = rows[cursor];
      flash = undefined;
      if (matchesKey(data, Key.escape) || data === 'q') return done(undefined);
      if (matchesKey(data, Key.up)) cursor = rows.length ? (cursor + rows.length - 1) % rows.length : 0;
      else if (matchesKey(data, Key.down)) cursor = rows.length ? (cursor + 1) % rows.length : 0;
      else if (matchesKey(data, Key.left) || matchesKey(data, Key.right)) {
        if (!current) return;
        // → steps toward done, ← back toward backlog: the same order the sections are drawn in.
        const next = ORDER[ORDER.indexOf(current.state) + (matchesKey(data, Key.right) ? 1 : -1)];
        if (!next) return;
        flash = source.move(current, next);
        reload(current.ref);
      } else if (matchesKey(data, Key.enter)) {
        if (current) return done({ action: 'open', ref: current.ref });
      } else if (data === 'a') return done({ action: 'add' });
      else if (data === 'e') return done({ action: 'export' });
      else if (data === 'd') {
        if (current) return done({ action: 'do', ref: current.ref });
      } else return;
      refresh();
    }

    function render(width: number): string[] {
      if (cache) return cache;
      const w = Math.max(20, width);
      const lines: string[] = [];
      const push = (text: string) => lines.push(truncateToWidth(text, w, '…'));
      const count = (state: BacklogState) => items.filter((item) => item.state === state).length;
      push(theme.fg('accent', '─'.repeat(w)));
      push(` ${theme.bold(sanitizeTerminalText(source.title))}  ${theme.fg('muted', `▶${count('ongoing')} ☐${count('todo')} ⧗${count('backlog')} ✓${count('done')}`)}`);
      const listed: string[] = [];
      let selectedLine = 0;
      for (const state of ORDER) {
        const group = rows.filter((item) => item.state === state);
        const total = count(state);
        listed.push('');
        listed.push(` ${theme.fg('accent', theme.bold(`${HEADINGS[state]} (${total})`))}${state === 'done' && total > group.length ? theme.fg('dim', ` · latest ${group.length}`) : ''}`);
        if (group.length === 0) listed.push(theme.fg('dim', '   (none)'));
        for (const item of group) {
          const selected = rows[cursor] === item;
          if (selected) selectedLine = listed.length;
          const owner = item.state === 'ongoing' && item.assignee ? (item.assignee === source.self ? ' (you)' : ` (@${sanitizeTerminalText(item.assignee)})`) : '';
          const tags = item.tags.length ? ` ${item.tags.map((tag) => `#${tag}`).join(' ')}` : '';
          const text = `${item.ref} ${priorityName(item.priority)} ${sanitizeTerminalText(item.title)}${theme.fg('dim', `${tags}${owner}`)}`;
          listed.push(selected ? `${theme.fg('accent', ' › ')}${theme.fg('accent', text)}` : `   ${text}`);
        }
      }
      // Keep the selection on screen when the board is taller than the terminal.
      const room = Math.max(6, (tui.terminal?.rows ?? 40) - 8);
      const start = listed.length <= room ? 0 : Math.min(Math.max(0, selectedLine - Math.floor(room / 2)), listed.length - room);
      for (const line of listed.slice(start, start + room)) push(line);
      lines.push('');
      if (flash) push(` ${theme.fg('warning', sanitizeTerminalText(flash))}`);
      push(` ${theme.fg('dim', '↑↓ select · ←→ move up/down a state · Enter actions · a add · d do it now · e export · Esc close')}`);
      push(theme.fg('accent', '─'.repeat(w)));
      cache = lines;
      return lines;
    }

    return { render, handleInput, invalidate: () => (cache = undefined) };
  });
}
