import type { ExtensionCommandContext } from '@earendil-works/pi-coding-agent';
import { wordCompletions, type Subcommand } from '../shared/commands.js';
import { errorMessage } from '../shared/util.js';
import { describeMembers } from '../team/routing.js';
import { USER_SENDER, type Team } from '../team/session.js';
import { mergeAgent, pendingRefs } from './worktree.js';

/** Running subagents by id (background, or foreground for `/agents kill`). `stoppedBy`: who stopped it (`user`, or the id of the agent that started it). */
export interface BackgroundRun {
  controller: AbortController;
  done: Promise<void>;
  stoppedBy?: string;
}
export type Background = Map<string, BackgroundRun>;

const AGENTS_USAGE = 'Usage: /agents [tell <agent|all> <message> | kill <id> | merge <id>]';
const AGENTS_SUBCOMMANDS: Array<[string, string]> = [
  ['tell', 'tell <agent|all> <message> — message an agent, or every agent of this session'],
  ['kill', 'kill <id> — stop a running subagent'],
  ['merge', 'merge <id> — merge an isolated subagent into this tree'],
];

/** Stops a running subagent (its whole process tree); its report then says who stopped it. */
export function stopBackground(background: Background, id: string, by: string, kind = 'background subagent'): string | undefined {
  const entry = background.get(id);
  if (!entry) {
    const ids = [...background.keys()];
    return `${id ? `No ${kind} "${id}".` : `Name the ${kind} to stop.`} ${ids.length ? `Running: ${ids.join(', ')}` : 'None is running.'}`;
  }
  entry.stoppedBy ??= by;
  entry.controller.abort();
  return undefined;
}

/**
 * `/octocode agents` (aliased as `/agents`) lists the agents; `tell`, `kill` and `merge` act on one. `kill` stops a
 * background subagent, or a foreground one (`foreground`) whose call then returns that the user stopped it.
 */
export function agentsCommand(team: Team, background: Background, foreground: Background = new Map()): Subcommand {
  const running = () => [...background.keys(), ...foreground.keys()];
  const tell = (args: string, ctx: ExtensionCommandContext) => {
    const to = args.split(/\s+/, 1)[0] ?? '';
    const text = args.slice(to.length).trim();
    if (!to || !text) return ctx.ui.notify('Usage: /agents tell <agent|all> <message>', 'warning');
    try {
      const result = team.send(to, text, { from: USER_SENDER, replyRequired: false });
      ctx.ui.notify('error' in result ? result.error : `Queued for ${result.sent.join(', ')}`, 'error' in result ? 'warning' : 'info');
    } catch (error) {
      ctx.ui.notify(errorMessage(error), 'warning');
    }
  };
  const kill = (id: string, ctx: ExtensionCommandContext) => {
    const ids = running();
    const refused = !id
      ? `Usage: /agents kill <id>. ${ids.length ? `Running: ${ids.join(', ')}` : 'None is running.'}`
      : foreground.has(id)
        ? stopBackground(foreground, id, USER_SENDER, 'subagent')
        : background.has(id) || ids.length === 0
          ? stopBackground(background, id, USER_SENDER, 'subagent')
          : `No running subagent "${id}". Running: ${ids.join(', ')}`;
    ctx.ui.notify(refused ?? `Stopping ${id}…`, refused ? 'warning' : 'info');
  };
  const merge = async (id: string, ctx: ExtensionCommandContext) => {
    if (!id) {
      const ids = await pendingRefs(ctx.cwd);
      return ctx.ui.notify(`Usage: /agents merge <id>. ${ids.length ? `Unmerged: ${ids.join(', ')}` : 'No isolated results to merge.'}`, 'warning');
    }
    try {
      const result = await mergeAgent(ctx.cwd, id);
      ctx.ui.notify(result.text, result.ok ? 'info' : 'error');
    } catch (error) {
      ctx.ui.notify(errorMessage(error), 'error');
    }
  };
  // Completions are synchronous: unmerged refs come from the last lookup, refreshed on every completion request.
  let cwd = process.cwd();
  let refs: string[] = [];
  let looking = false;
  const refreshRefs = () => {
    if (looking) return;
    looking = true;
    void pendingRefs(cwd)
      .then((found) => (refs = found), () => undefined)
      .finally(() => (looking = false));
  };
  const ids = (sub: string): string[] => {
    if (sub === 'kill') return running();
    if (sub === 'merge') {
      refreshRefs();
      return refs;
    }
    const members = team.view.members.length > 0 ? team.view.members : team.members();
    return [...members.map((member) => member.id).filter((id) => id !== team.id), 'all'];
  };
  return {
    description: 'agents [tell <agent|all> <message> | kill <id> | merge <id>] — list agents, message or stop one',
    complete: (prefix) => {
      const space = prefix.indexOf(' ');
      if (space === -1) return wordCompletions(AGENTS_SUBCOMMANDS, prefix)?.map((item) => ({ ...item, value: `${item.value} ` })) ?? null;
      const sub = prefix.slice(0, space);
      const rest = prefix.slice(space + 1).trimStart();
      if (!AGENTS_SUBCOMMANDS.some(([name]) => name === sub) || /\s/.test(rest)) return null;
      const items = wordCompletions(ids(sub), rest);
      return items?.map((item) => ({ ...item, value: `${sub} ${item.value}${sub === 'tell' ? ' ' : ''}` })) ?? null;
    },
    handler: async (args, ctx) => {
      cwd = ctx.cwd;
      const line = args.trim();
      const sub = line.split(/\s+/, 1)[0] ?? '';
      const rest = line.slice(sub.length).trim();
      if (sub === 'tell') return tell(rest, ctx);
      if (sub === 'kill') return kill(rest, ctx);
      if (sub === 'merge') return merge(rest, ctx);
      if (sub) return ctx.ui.notify(AGENTS_USAGE, 'warning');
      const { members, error } = team.snapshot();
      ctx.ui.notify(error ? `${error}\n${describeMembers(members, team.id)}` : describeMembers(members, team.id), error ? 'warning' : 'info');
    },
  };
}
