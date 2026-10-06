import fs from 'node:fs';
import path from 'node:path';
import { SessionManager, type ExtensionAPI, type ExtensionCommandContext, type SessionInfo } from '@earendil-works/pi-coding-agent';
import { say, wordCompletions, type Subcommands } from '../shared/commands.js';
import { sessionDir } from '../shared/home.js';
import { processAlive } from '../shared/process.js';
import { contentText, errorMessage } from '../shared/util.js';
import { shortDuration } from './brief.js';
import { oneLine, type SessionExtra, type SessionIndex } from './store.js';

/** Most sessions listed. */
const LIST_LIMIT = 30;

const ACTIONS = { resume: 'Resume', details: 'Details', rename: 'Rename', forget: 'Forget Octocode data' } as const;

/** Random tail of a session id (Pi's UUIDv7 ids share their leading timestamp digits). */
const shortId = (id: string): string => id.replace(/[^\w]/g, '').slice(-6);

/** A session as listed: Pi's view of it, with what the extras table adds (cost, branch, files, liveness). */
interface ListedSession {
  info: SessionInfo;
  extra: SessionExtra | undefined;
  /** The first message of the session it was forked from. */
  parentPrompt?: string;
}

/** Whether a session's process is running now (this one included). */
const live = ({ extra }: ListedSession): boolean => !!extra?.pid && processAlive(extra.pid);

function title({ info, parentPrompt }: ListedSession): string {
  // A fork has no prompt of its own until its first new one: show what it was forked from.
  return oneLine(info.name || info.firstMessage || (parentPrompt ? `↳ ${parentPrompt}` : '(no prompt yet)'), 60);
}

/** `● name-or-first-prompt · cwd · branch · 2h ago · $1.84 · 7 files · 12 messages · 3f9a2c`, `(current)` for this session. */
function sessionLine(row: ListedSession, current: string | undefined, all: boolean, now = Date.now()): string {
  const { info, extra } = row;
  const parts = [`${live(row) ? '●' : '○'} ${title(row)}`];
  if (all) parts.push(oneLine(path.basename(info.cwd) || info.cwd || '?', 40));
  if (extra?.branch) parts.push(oneLine(extra.branch, 40));
  parts.push(`${shortDuration(now - info.modified.getTime())} ago`);
  if (extra && extra.cost > 0) parts.push(`$${extra.cost.toFixed(2)}`);
  if (extra && extra.files_changed > 0) parts.push(`${extra.files_changed} file${extra.files_changed === 1 ? '' : 's'}`);
  parts.push(`${info.messageCount} message${info.messageCount === 1 ? '' : 's'}`, shortId(info.id));
  return `${parts.join(' · ')}${info.id === current ? ' (current)' : ''}`;
}

function details(row: ListedSession, current: string | undefined): string {
  const { info, extra } = row;
  const at = (date: Date) => date.toISOString().slice(0, 16).replace('T', ' ');
  return [
    sessionLine(row, current, true),
    `id: ${info.id}`,
    `file: ${info.path}`,
    `cwd: ${oneLine(info.cwd, 300)}`,
    `started: ${at(info.created)} UTC · last active: ${at(info.modified)} UTC`,
    ...(info.parentSessionPath ? [`forked from: ${info.parentSessionPath}`] : []),
    ...(info.firstMessage && info.name ? [`first prompt: ${oneLine(info.firstMessage, 300)}`] : []),
    `tokens: ${extra?.tokens ?? 0} · live: ${live(row) ? `yes (pid ${extra?.pid})` : 'no'}`,
    `octocode data: ${sessionDir(info.id)}`,
  ].join('\n');
}

/** Pi's sessions, newest first, matching `query` (name, messages, id, branch), with their extras. */
/** This session as Pi would list it: it has no file before its first reply (or ever, with --no-session). */
function currentInfo(ctx: ExtensionCommandContext): SessionInfo {
  const manager = ctx.sessionManager;
  const messages = manager.getEntries().filter((entry) => entry.type === 'message');
  const first = messages.find((entry) => entry.message.role === 'user');
  const header = manager.getHeader();
  return {
    path: manager.getSessionFile() ?? '',
    id: manager.getSessionId(),
    cwd: ctx.cwd,
    ...(manager.getSessionName() ? { name: manager.getSessionName() } : {}),
    created: new Date(header?.timestamp ?? Date.now()),
    modified: new Date(),
    messageCount: messages.length,
    firstMessage: first?.type === 'message' ? contentText((first.message as { content?: unknown }).content) : '',
    allMessagesText: '',
  };
}

async function listSessions(ctx: ExtensionCommandContext, index: SessionIndex | undefined, all: boolean, query: string): Promise<ListedSession[]> {
  const infos = all ? await SessionManager.listAll() : await SessionManager.list(ctx.cwd, ctx.sessionManager.getSessionDir());
  if (!infos.some((info) => info.id === ctx.sessionManager.getSessionId())) infos.push(currentInfo(ctx));
  const extras = index?.extras(infos.map((info) => info.id)) ?? new Map<string, SessionExtra>();
  const needle = query.toLowerCase();
  const firstByPath = new Map(infos.map((info) => [path.resolve(info.path), info.firstMessage]));
  return infos
    .map((info) => ({ info, extra: extras.get(info.id) }))
    .filter(({ info, extra }) => !needle || [info.name, info.firstMessage, info.allMessagesText, info.id, extra?.branch].some((text) => text?.toLowerCase().includes(needle)))
    .sort((a, b) => b.info.modified.getTime() - a.info.modified.getTime())
    .slice(0, LIST_LIMIT)
    .map((row) => {
      const parentPrompt = row.info.parentSessionPath ? firstByPath.get(path.resolve(row.info.parentSessionPath)) : undefined;
      return parentPrompt ? { ...row, parentPrompt } : row;
    });
}

interface SessionsCommandOptions {
  /** The index, or throws when the agent database cannot be opened. */
  index: () => SessionIndex;
  /** The current session id. */
  current: () => string | undefined;
}

/** `/octocode sessions [all|<query>]` (and `/sessions`): list, then resume, inspect, rename or forget one. */
export function registerSessionsCommand(pi: ExtensionAPI, commands: Subcommands, options: SessionsCommandOptions): void {
  commands.add('sessions', {
    description: 'sessions [all|<query>] — recent sessions of this directory: resume, details, rename, forget',
    complete: (prefix) => wordCompletions(['all'], prefix),
    handler: async (args, ctx) => {
      // Without the agent database the list still works; it only lacks cost, branch, files and liveness.
      let index: SessionIndex | undefined;
      try {
        index = options.index();
      } catch {
        index = undefined;
      }
      const all = args === 'all';
      const query = all ? '' : args.trim();
      const current = options.current();
      let rows: ListedSession[];
      try {
        rows = await listSessions(ctx, index, all, query);
      } catch (error) {
        return say(ctx, `Pi could not list its sessions: ${errorMessage(error)}`, 'warning');
      }
      const scope = all ? 'all directories' : `${oneLine(path.basename(ctx.cwd) || ctx.cwd, 60)}${query ? ` matching "${oneLine(query, 60)}"` : ''}`;
      if (rows.length === 0) return say(ctx, `No saved sessions for ${scope}.${all ? '' : ' Try /sessions all.'}`);
      const lines = uniqueLines(rows.map((row) => sessionLine(row, current, all)));
      if (!ctx.hasUI) return say(ctx, [`Sessions (${scope}), newest first:`, ...lines.map((line, i) => `${line}\n    ${rows[i]!.info.id}`)].join('\n'));
      const picked = await ctx.ui.select(`Sessions — ${scope} (● running)`, lines);
      const row = picked === undefined ? undefined : rows[lines.indexOf(picked)];
      if (!row) return;
      // Pi owns session names and can only rename the session it has open.
      const actions = Object.values(ACTIONS).filter((action) => action !== ACTIONS.rename || row.info.id === current);
      const action = await ctx.ui.select(title(row), actions);
      if (action === ACTIONS.resume) return resume(ctx, row, current);
      if (action === ACTIONS.details) return say(ctx, details(row, current));
      if (action === ACTIONS.rename) return rename(pi, ctx, row, current);
      if (action === ACTIONS.forget) return forget(ctx, index, row, current);
    },
  });
  commands.alias(pi, 'sessions');
}

/** `select` returns the chosen text, so every line must differ: a repeat (same id tail) gets ` #2`, ` #3`, … */
function uniqueLines(lines: string[]): string[] {
  const seen = new Map<string, number>();
  return lines.map((line) => {
    const count = (seen.get(line) ?? 0) + 1;
    seen.set(line, count);
    return count === 1 ? line : `${line} #${count}`;
  });
}

async function resume(ctx: ExtensionCommandContext, row: ListedSession, current: string | undefined): Promise<void> {
  if (row.info.id === current) return ctx.ui.notify('That is the current session.', 'info');
  if (live(row) && !(await ctx.ui.confirm('Session is running', `Another Pi process (pid ${row.extra?.pid}) has this session open. Resume it here as well?`))) return;
  await ctx.switchSession(row.info.path);
}

async function rename(pi: ExtensionAPI, ctx: ExtensionCommandContext, row: ListedSession, current: string | undefined): Promise<void> {
  if (row.info.id !== current) return;
  const name = oneLine((await ctx.ui.input('New session name', row.info.name ?? '')) ?? '', 200);
  if (!name) return;
  pi.setSessionName(name);
  ctx.ui.notify(`Renamed to "${name}".`, 'info');
}

async function forget(ctx: ExtensionCommandContext, index: SessionIndex | undefined, row: ListedSession, current: string | undefined): Promise<void> {
  if (row.info.id === current) return ctx.ui.notify('The current session cannot be forgotten while it runs.', 'warning');
  // Its process would keep writing output, logs and checkpoints into the folder, and its extras would come back.
  if (live(row) && row.extra?.pid !== process.pid) return ctx.ui.notify(`That session is open in another Pi process (pid ${row.extra?.pid}); close it there first.`, 'warning');
  const ok = await ctx.ui.confirm('Forget Octocode data', `Delete Octocode's saved output, bash logs, checkpoints and extras for "${title(row)}"? Pi's session file stays.`);
  if (!ok) return;
  fs.rmSync(sessionDir(row.info.id), { recursive: true, force: true });
  index?.forget(row.info.id);
  ctx.ui.notify(`Forgot Octocode data for "${title(row)}".`, 'info');
}
