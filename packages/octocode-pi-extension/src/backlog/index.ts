import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import type { Subcommands } from '../shared/commands.js';
import { backlogCommand } from './command.js';
import { openBacklog, type BacklogOptions } from './context.js';
import { countsText } from './format.js';
import { registerBacklogTool } from './tool.js';

const BACKLOG_STATUS_KEY = 'octocode-backlog';
interface RegisterBacklogOptions extends BacklogOptions {
  commands: Subcommands;
}

/**
 * The repository backlog: the `backlog` tool, `/octocode backlog` (alias it with `commands.alias(pi, 'backlog')` after
 * `commands` is registered) and footer counts in words with the `/backlog` hint that opens the board.
 */
export function registerBacklog(pi: ExtensionAPI, options: RegisterBacklogOptions): void {
  const { commands, ...backlogOptions } = options;

  const refresh = (ctx: ExtensionContext) => {
    if (!ctx.hasUI || options.isSubagent) return;
    try {
      const backlog = openBacklog(ctx, backlogOptions);
      const text = countsText(backlog.store.counts());
      ctx.ui.setStatus(BACKLOG_STATUS_KEY, text || undefined);
    } catch {
      // An unreadable agent database (foreign file, newer schema) hides the counts; the tool and command report why.
      ctx.ui.setStatus(BACKLOG_STATUS_KEY, undefined);
    }
  };

  registerBacklogTool(pi, { ...backlogOptions, changed: refresh });
  if (!options.isSubagent) commands.add('backlog', backlogCommand(pi, { ...backlogOptions, changed: refresh }));
  pi.on('session_start', (_event, ctx) => refresh(ctx));
  // Another session or agent may have changed the board: repaint at the end of every run.
  pi.on('agent_end', (_event, ctx) => refresh(ctx));
}
