import { sanitizeTerminalText } from '../shared/sanitize.js';
import type { ExtensionAPI, ExtensionContext, Theme } from '@earendil-works/pi-coding-agent';
import { truncateToWidth, type Component } from '@earendil-works/pi-tui';
import type { Subcommands } from '../shared/commands.js';
import { backlogCommand } from './command.js';
import { openBacklog, type BacklogOptions } from './context.js';
import { countsText } from './format.js';
import { registerBacklogTool } from './tool.js';

const BACKLOG_STATUS_KEY = 'octocode-backlog';
const BACKLOG_WIDGET_KEY = 'octocode-backlog';
const WIDGET_MAX = 3;

type Row = { ref: string; title: string };

/** Plain widget rows: the ongoing items (titles sanitized), then how many more there are. */
function widgetRows(mine: readonly Row[]): Array<{ ref?: string; text: string }> {
  return [...mine.slice(0, WIDGET_MAX).map((item) => ({ ref: item.ref, text: sanitizeTerminalText(item.title).replace(/\s+/g, ' ').trim() })), ...(mine.length > WIDGET_MAX ? [{ text: `+${mine.length - WIDGET_MAX} more ongoing` }] : [])];
}

/** The below-editor widget, styled like the agents panel: accent marker and ref, muted titles, one line each. */
export function backlogWidget(mine: readonly Row[]): (tui: unknown, theme: Theme) => Component {
  const rows = widgetRows(mine);
  return (_tui, theme) => ({
    render: (width: number) =>
      rows.map((row) => truncateToWidth(row.ref ? ` ${theme.fg('accent', '▶')} ${theme.fg('accent', theme.bold(row.ref))} ${theme.fg('muted', row.text)}` : `   ${theme.fg('dim', row.text)}`, Math.max(1, width), '…')),
    invalidate: () => undefined,
  });
}

interface RegisterBacklogOptions extends BacklogOptions {
  commands: Subcommands;
}

/**
 * The repository backlog: the `backlog` tool, `/octocode backlog` (alias it with `commands.alias(pi, 'backlog')` after
 * `commands` is registered), footer counts (▶ongoing ☐todo ⧗awaiting triage) and a widget naming the items this
 * session has ongoing.
 */
export function registerBacklog(pi: ExtensionAPI, options: RegisterBacklogOptions): void {
  const { commands, ...backlogOptions } = options;

  const refresh = (ctx: ExtensionContext) => {
    if (!ctx.hasUI || options.isSubagent) return;
    try {
      const backlog = openBacklog(ctx, backlogOptions);
      const text = countsText(backlog.store.counts());
      ctx.ui.setStatus(BACKLOG_STATUS_KEY, text || undefined);
      const mine = backlog.store.list({ states: ['ongoing'] }).items.filter((item) => item.assignee === backlog.session);
      if (mine.length === 0) ctx.ui.setWidget(BACKLOG_WIDGET_KEY, undefined);
      // RPC clients ignore component factories: they get the plain rows.
      else if (ctx.mode === 'tui') ctx.ui.setWidget(BACKLOG_WIDGET_KEY, backlogWidget(mine), { placement: 'belowEditor' });
      else ctx.ui.setWidget(BACKLOG_WIDGET_KEY, widgetRows(mine).map((row) => (row.ref ? `▶ ${row.ref} ${row.text}` : `  ${row.text}`)), { placement: 'belowEditor' });
    } catch {
      // An unreadable agent database (foreign file, newer schema) hides the counts and the widget; the tool and command report why.
      ctx.ui.setStatus(BACKLOG_STATUS_KEY, undefined);
      ctx.ui.setWidget(BACKLOG_WIDGET_KEY, undefined);
    }
  };

  registerBacklogTool(pi, { ...backlogOptions, changed: refresh });
  if (!options.isSubagent) commands.add('backlog', backlogCommand(pi, { ...backlogOptions, changed: refresh }));
  pi.on('session_start', (_event, ctx) => refresh(ctx));
  // Another session or agent may have changed the board: repaint at the end of every run.
  pi.on('agent_end', (_event, ctx) => refresh(ctx));
}
