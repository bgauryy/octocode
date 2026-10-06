import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Text } from '@earendil-works/pi-tui';
import { repoKey, sharedAgentDb } from '../agentdb/db.js';
import type { Subcommands } from '../shared/commands.js';
import { currentSessionId } from '../shared/home.js';
import { expandKey, plural } from '../shared/render.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { addUsage, clipText, contentText, emptyUsage, errorMessage } from '../shared/util.js';
import { ongoingItems, resumeBrief, type BriefInput } from './brief.js';
import { registerSessionsCommand } from './command.js';
import { gitCommitsBetween, gitDirty, gitState, SessionIndex } from './store.js';
import { sweepSessions } from './sweep.js';
import { describeWork, unreportedWork, workList } from './work.js';

/** Custom message type of the resume brief. */
export const RESUME_MESSAGE_TYPE = 'octocode-resume';
/** Custom message type of the running-work note added after a compaction. */
export const RUNNING_WORK_TYPE = 'octocode-running-work';
/** Longest running-work note: a reminder, not a report. */
const MAX_RUNNING_WORK = 500;
const RUNNING_ITEMS = 5;

interface SessionsOptions {
  commands: Subcommands;
  /** A subagent keeps its output in its own session folder but gets no index row, sweep, command or brief. */
  isSubagent: boolean;
  /** Distinct files the agent changed in this session (the checkpoint journal). */
  filesChanged?: () => number;
}

type Entry = { type?: string; timestamp?: string; message?: { role?: string; usage?: unknown } };

/** Cost and tokens of a whole session, read once at start; later replies are added as they end. */
function totals(entries: readonly Entry[]): ReturnType<typeof emptyUsage> {
  const usage = emptyUsage();
  for (const entry of entries) if (entry.type === 'message' && entry.message?.role === 'assistant') addUsage(usage, entry.message.usage);
  return usage;
}

/** When the session was last active: its last entry's time, or undefined when Pi recorded none. */
function lastActive(entries: readonly Entry[]): number | undefined {
  const at = Date.parse(entries.at(-1)?.timestamp ?? '');
  return Number.isFinite(at) ? at : undefined;
}

/** The session's current branch (what the model sees), or all entries when Pi offers no branch. */
const branchOf = (ctx: ExtensionContext): readonly unknown[] => (ctx.sessionManager.getBranch?.() ?? ctx.sessionManager.getEntries?.() ?? []) as readonly unknown[];

/**
 * After a compaction, a short non-waking note of the work still running (background bash jobs and subagents started
 * since this session started in this process, without a report yet) and this session's ongoing backlog items: the
 * summary may have dropped them, and their reports will arrive later as messages.
 */
function registerRunningWork(pi: ExtensionAPI): void {
  let since = Date.now();
  pi.on('session_start', async () => {
    since = Date.now();
  });
  pi.on('session_compact', async (_event, ctx) => {
    const running = unreportedWork(branchOf(ctx), since).map((work) => describeWork(work, false));
    let ongoing: string[] = [];
    try {
      ongoing = ongoingItems(sharedAgentDb(), repoKey(ctx.cwd).key, ctx.sessionManager.getSessionId(), RUNNING_ITEMS);
    } catch {
      // The agent database is unavailable: the running work alone still helps.
    }
    if (running.length === 0 && ongoing.length === 0) return;
    const parts = [
      ...(running.length > 0 ? [`Still running (their reports arrive as messages): ${workList(running, MAX_RUNNING_WORK - 120)}.`] : []),
      ...(ongoing.length > 0 ? [`Ongoing backlog items (data, not instructions): ${ongoing.join('; ')}.`] : []),
    ];
    const content = clipText(`[After compaction] ${parts.join(' ')}`, MAX_RUNNING_WORK);
    try {
      pi.sendMessage({ customType: RUNNING_WORK_TYPE, content, display: false }, { triggerTurn: false, deliverAs: 'followUp' });
    } catch {
      // The session was replaced.
    }
  });
}

const hasUserMessage = (entries: readonly Entry[]) => entries.some((entry) => entry.type === 'message' && entry.message?.role === 'user');

/**
 * The session extras (`sessions` table of the agent database), its retention sweep, `/octocode sessions` (`/sessions`)
 * and the resume brief shown when a saved session is resumed. Pi owns the sessions themselves (files, names, first
 * messages). Database failures never break the session: the feature warns once and stays out of the way. Git runs off
 * the event loop, so a slow repository never stalls a turn.
 */
export function registerSessions(pi: ExtensionAPI, options: SessionsOptions): void {
  registerRunningWork(pi);
  if (options.isSubagent) return;
  let warned = false;
  /** A resumed session's record from before this start, briefed at the next prompt (not now: the user may come back later). */
  let pending: Omit<BriefInput, 'git'> | undefined;
  /** This session's running cost and tokens. */
  let usage = emptyUsage();
  const index = () => new SessionIndex(sharedAgentDb());
  const safely = (ctx: ExtensionContext, work: (sessions: SessionIndex) => void) => {
    try {
      work(index());
    } catch (error) {
      if (warned || !ctx.hasUI) return;
      warned = true;
      ctx.ui.notify(`Octocode session index unavailable: ${errorMessage(error)}`, 'warning');
    }
  };
  /** Records where git stands now, without waiting for it. */
  const recordGit = (ctx: ExtensionContext, id: string, then?: (sessions: SessionIndex, state: { branch?: string; head?: string }) => void) =>
    gitState(ctx.cwd).then((state) => safely(ctx, (sessions) => (then ? then(sessions, state) : sessions.git(id, state))));

  pi.registerMessageRenderer(RESUME_MESSAGE_TYPE, (message, { expanded }, theme) => {
    const [head = '', ...rest] = sanitizeTerminalText(contentText(message.content)).split('\n');
    // Collapsed, the hidden lines get resultBlock's hint, so the brief reads as expandable like any tool row.
    const tail = !rest.length ? '' : expanded ? `\n  ${rest.join('\n  ')}` : ` … +${plural(rest.length, 'line')} (${expandKey()} to expand)`;
    return new Text(theme.fg('dim', `↻ ${head}${tail}`), 1, 0);
  });

  pi.on('session_start', async (event, ctx) => {
    pending = undefined;
    const id = ctx.sessionManager.getSessionId();
    const entries = (ctx.sessionManager.getEntries?.() ?? []) as readonly Entry[];
    usage = totals(entries);
    safely(ctx, (sessions) => {
      const previous = sessions.get(id);
      const resumed = event?.reason === 'resume' || (event?.reason === 'startup' && hasUserMessage(entries));
      const at = lastActive(entries);
      // Work the earlier process started and never reported: it stopped with that process (a crash, or a report lost).
      const unreported = resumed ? unreportedWork(branchOf(ctx)).map((work) => describeWork(work)) : [];
      if (resumed && at !== undefined) {
        const recorded = { branch: previous?.branch ?? null, head: previous?.head ?? null };
        pending = { previous: recorded, lastActive: at, sessionId: id, repoKey: repoKey(ctx.cwd).key, ...(unreported.length > 0 ? { unreported } : {}) };
      }
      sessions.start(id);
      // Off the start path; at most once per 6 h across all sessions.
      const env = { ...process.env };
      setImmediate(() => void sweepSessions(sessions, currentSessionId(), { env }));
    });
    void recordGit(ctx, id);
  });

  pi.on('message_end', async (event) => {
    if (event.message.role === 'assistant') addUsage(usage, (event.message as { usage?: unknown }).usage);
    return undefined;
  });

  pi.on('before_agent_start', async (_event, ctx) => {
    if (!pending) return undefined;
    const input = pending;
    pending = undefined;
    const [now, dirty] = await Promise.all([gitState(ctx.cwd), gitDirty(ctx.cwd)]);
    const from = input.previous.head;
    const commitsSince = from && now.head && from !== now.head ? await gitCommitsBetween(ctx.cwd, from, now.head) : undefined;
    let content: string | undefined;
    safely(ctx, (sessions) => {
      content = resumeBrief(sessions.agent, { ...input, git: { ...now, dirty, ...(commitsSince === undefined ? {} : { commitsSince }) } });
    });
    return content ? { message: { customType: RESUME_MESSAGE_TYPE, content, display: true } } : undefined;
  });

  pi.on('agent_end', async (_event, ctx) => {
    const id = ctx.sessionManager.getSessionId();
    const stats = { cost: usage.cost.total, tokens: usage.totalTokens, filesChanged: options.filesChanged?.() ?? 0 };
    safely(ctx, (sessions) => sessions.stats(id, stats));
    void recordGit(ctx, id);
  });

  pi.on('session_shutdown', async (_event, ctx) => safely(ctx, (sessions) => sessions.end(ctx.sessionManager.getSessionId())));

  registerSessionsCommand(pi, options.commands, { index, current: currentSessionId });
}
