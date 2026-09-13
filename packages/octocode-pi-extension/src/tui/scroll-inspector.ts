import { Input, matchesKey, wrapTextWithAnsi } from '@earendil-works/pi-tui';
import type { PiContext } from '../types.js';
import { paint } from './palette.js';
import { truncateToWidth } from './width.js';

/** A command-owned scroll view; every wrapped line stays reachable with the keyboard. */
export async function openScrollInspector(
  ctx: PiContext,
  title: string,
  lines: readonly string[]
): Promise<void> {
  if (ctx.mode !== 'tui' || !ctx.hasUI || !ctx.ui?.custom) {
    ctx.ui?.notify?.('This view needs an interactive terminal.', 'warning');
    return;
  }
  await ctx.ui.custom<void>((tui, theme, _keys, done) => {
    let offset = 0;
    let wrapped: string[] = [];
    let cachedWidth = -1;
    let pageSize = 14;
    const search = new Input();
    let searching = false;
    let query = '';
    let cachedQuery = '';
    const requestRender = () => (tui as { requestRender?: () => void }).requestRender?.();
    const clearSearch = () => {
      searching = false;
      query = '';
      search.setValue('');
      offset = 0;
      requestRender();
    };
    search.onSubmit = () => { searching = false; requestRender(); };
    search.onEscape = clearSearch;
    return {
      invalidate() {
        cachedWidth = -1;
      },
      render(width: number) {
        const terminal = tui as { terminal?: { rows?: number } };
        pageSize = Math.max(
          1,
          Math.min(18, (terminal.terminal?.rows ?? 24) - 6)
        );
        if (cachedWidth !== width || cachedQuery !== query) {
          wrapped = lines.filter(line => line.toLocaleLowerCase().includes(query.toLocaleLowerCase())).flatMap(line =>
            wrapTextWithAnsi(line, Math.max(1, width))
          );
          cachedWidth = width;
          cachedQuery = query;
        }
        offset = Math.max(
          0,
          Math.min(offset, Math.max(0, wrapped.length - pageSize))
        );
        return [
          paint(theme, 'title', title),
          ...(searching
            ? [`/ ${search.render(Math.max(1, width - 2))[0] ?? ''}`]
            : query ? [paint(theme, 'dim', `Filter: ${query} · Esc clear`)] : []),
          ...(query && wrapped.length === 0 ? [paint(theme, 'muted', 'No matches. Esc clears the filter.')] : []),
          ...wrapped.slice(offset, offset + pageSize),
          paint(
            theme,
            'dim',
            `Esc ${query ? 'clear' : 'close'} · / filter · ${wrapped.length ? offset + 1 : 0}–${Math.min(wrapped.length, offset + pageSize)}/${wrapped.length} · ↑↓ · PgUp/PgDn`
          ),
        ].map(line => truncateToWidth(line, width));
      },
      handleInput(data: string) {
        if (matchesKey(data, 'ctrl+c')) { done(); return; }
        if (searching) {
          search.handleInput(data);
          const next = search.getValue();
          if (query !== next) { query = next; offset = 0; }
          requestRender();
          return;
        }
        if (data === '/') {
          searching = true;
          search.focused = true;
          requestRender();
          return;
        }
        if (matchesKey(data, 'escape') && query) { clearSearch(); return; }
        if (matchesKey(data, 'escape') || matchesKey(data, 'ctrl+c')) {
          done();
          return;
        }
        if (matchesKey(data, 'up')) offset -= 1;
        else if (matchesKey(data, 'down')) offset += 1;
        else if (matchesKey(data, 'pageUp')) offset -= pageSize;
        else if (matchesKey(data, 'pageDown')) offset += pageSize;
        else if (data === 'g') offset = 0;
        else if (data === 'G') offset = wrapped.length;
        offset = Math.max(
          0,
          Math.min(offset, Math.max(0, wrapped.length - pageSize))
        );
        requestRender();
      },
    };
  });
}
