import type { Usage } from '@earendil-works/pi-ai';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';
import { existingLibrary } from '../shared/archive.js';
import { formatCost, formatDuration, shortPath, toolCallCount } from '../shared/format.js';
import { addReply, emptyTotals, errorMessage, firstLine, modelLookup, settleWithin, unpricedModel, type ModelLookup } from '../shared/util.js';
import { UserQuestions } from '../team/asks.js';
import { AgentsView } from '../team/panel.js';
import { checkGoal, GOAL_DESCRIPTION } from '../team/identity.js';
import type { Team } from '../team/session.js';
import { USER_SENDER } from '../shared/messages.js';
import { timedTool } from '../shared/tool-timing.js';
import type { Permission } from '../permission/ask.js';
import { capitalize, compactionLabel, deniedLabel, doneSummary, noticeLines, renderAgentCall, renderAgentMessage, renderAgentResult, type ResultDetails, type RunDetails } from './render.js';
import { claimRun, joinTeam, lingerSeconds, maxSubagents, planRun, Slots, withIdentity, type AgentRuns, type Run } from './claim.js';
import type { ChildProgress } from './events.js';
import { runForeground } from './foreground.js';
import { Isolation } from './isolation.js';
import { LateReports } from './late.js';
import { childSession, type ChildSession, type Identity } from './launch.js';
import { Lingering } from './linger.js';
import { buildAgentArgs, runSubagent, SUBAGENT_STOP_GRACE_MS } from './process.js';
import { askSpawn, type AgentProfile } from './profiles.js';
import { pendingReportsDir, ReadReports, ReportQueue, RESULT_TYPE, savePendingReport, scratchRoot, sweepScratch, type Report } from './handoff.js';
import { reportStatus } from './report.js';
import { FAILED_NEXT, PARTIAL_UNVERIFIED, PERMISSION_STOP, recordRun, reportMessage, runDetails, runFooter, waitingUpdate, type Outcome } from './run-details.js';
import { saveShot, shotsDir, type Shot } from './screenshots.js';
import { agentsCommand, stopBackground, type BackgroundRun } from './command.js';
import type { RelayHost } from './relay.js';
import type { Subcommand } from '../shared/commands.js';
import { notify } from '../shared/commands.js';
import { describePrune, pendingRefs, pruneWorktrees, recordWorktreePid } from './worktree.js';

/** What the rest of the extension may do with the subagents this session started. */
export interface AgentControl {
  /** Stops a background subagent this session started; the refusal text when there is none by that id. */
  stop(id: string, by: string): string | undefined;
  /** Resolves once every background subagent has finished and sent its report. */
  settle(): Promise<unknown>;
  /** Background subagents running now (a session switch would stop them). */
  background(): number;
  /**
   * Session shutdown's first step: stops every background subagent and waits (bounded) for their reports. Idempotent.
   * `reason`: the shutdown's (`reload`, `quit`…), named in the reports of the runs it stopped.
   */
  stopAll(reason?: string): Promise<void>;
  /** `/octocode agents`: list agents, tell, kill, merge; `index.ts` registers it and its `/agents` shortcut. */
  command: Subcommand;
}

/** How long shutdown waits for stopped background subagents: past the SIGTERM grace, after which they are killed. */
const SHUTDOWN_WAIT_MS = SUBAGENT_STOP_GRACE_MS + 1_000;

type AgentUpdate = (update: { content: Array<{ type: 'text'; text: string } | { type: 'image'; data: string; mimeType: string }>; details: RunDetails }) => void;

/** Trail lines a run keeps (tool calls, thoughts, narration); the collapsed row previews the last three. */
const ACTIVITY_KEPT = 8;

/** The row's line while a child waits for the user's answer to one of its dialogs. */
const WAITING_TEXT = 'waiting for your approval';

/** The row's line while a child compacts its context. */
const COMPACTING_TEXT = 'compacting its context';

/** A run's total usage so far (Pi's `Usage`, kept up to date by the child's event stream), by its details. */
const spent = new WeakMap<RunDetails, Usage>();

/** What a background report carries of the run besides its text: its spend (the parent's footer and session index count it), its compactions and notices, its saved conversation, its questions to the parent. */
const reportFacts = (details: RunDetails): Pick<ResultDetails, 'usage' | 'unpriced' | 'compactions' | 'notices' | 'session' | 'asks'> => {
  const usage = spent.get(details);
  return {
    ...(usage ? { usage, unpriced: details.unpriced } : {}),
    ...(details.compactions ? { compactions: details.compactions } : {}),
    ...(details.notices ? { notices: details.notices } : {}),
    ...(details.session ? { session: details.session } : {}),
    ...(details.asks ? { asks: details.asks } : {}),
  };
};

/**
 * Progress of a run into `details` (tool lines, usage, the latest screenshot), mirrored to `onUpdate`. A reply from a model
 * without pricing (looked up in this session's registry) marks the run's cost `n/a`.
 */
function tracker(identity: Identity, details: RunDetails, find: ModelLookup) {
  const dir = shotsDir();
  let shots = 0;
  let latest: Shot | undefined;
  return (onUpdate: AgentUpdate | undefined) => ({ activity: line, step, thinking, usage, reply, image, denied, waiting, compacting, compactions, notices }: ChildProgress) => {
    if (waiting !== undefined) {
      if (waiting) details.waiting = true;
      else delete details.waiting;
    }
    if (compacting !== undefined) {
      if (compacting) details.compacting = true;
      else delete details.compacting;
    }
    if (compactions) details.compactions = compactions;
    if (notices) details.notices = notices;
    if (denied) details.denied = (details.denied ?? 0) + 1;
    if (line) details.toolCalls += 1;
    const entry = line ?? step;
    if (entry) details.activity = [...details.activity.slice(-(ACTIVITY_KEPT - 1)), entry];
    if (thinking !== undefined) {
      if (thinking) details.thinking = thinking;
      else delete details.thinking;
    }
    if (image) {
      shots += 1;
      latest = saveShot(dir, identity.id, shots, image);
      details.shot = latest.path;
    }
    // `usage` is the run's total so far: the details restart from it. Cached prompt tokens count in ↑.
    const unpriced = details.unpriced || unpricedModel(reply, find) !== undefined;
    spent.set(details, usage);
    Object.assign(details, emptyTotals());
    addReply(details, usage, unpriced);
    const text = { type: 'text' as const, text: details.waiting ? WAITING_TEXT : details.compacting ? COMPACTING_TEXT : line ?? details.activity.at(-1) ?? 'working' };
    // Partial results only reach the user's screen, so the latest screenshot is drawn there but never enters the model's context.
    onUpdate?.({ content: image && latest?.data ? [text, { type: 'image', data: latest.data, mimeType: latest.mimeType }] : [text], details: { ...details } });
  };
}

/** Runs the subagent in the background; its report (or why it stopped) arrives as a message, and then it gives its slot back. */
function startBackground(
  runs: AgentRuns, run: Run, details: RunDetails, task: string, launch: (signal: AbortSignal) => Promise<Outcome>, finish: () => Promise<string>, piSession: string,
  linger?: () => string,
): void {
  const { background, read } = runs;
  const { identity } = run;
  const session = runs.session;
  // A report that arrives after its session ended (a reload, a switch) is kept for the next start of that Pi session.
  const reports = { add: (report: Report) => (session === runs.session ? runs.reports.add(report) : savePendingReport(pendingReportsDir(piSession), identity.id, report)) };
  const controller = new AbortController();
  const entry: BackgroundRun = { controller, done: Promise.resolve() };
  background.set(identity.id, entry);
  runs.changed();
  read.track(identity.id, identity.scratch);
  // The parent may have many runs going: every report names the agent, its role and its goal.
  const who = `${identity.id} (${identity.role}: ${identity.goal})`;
  // For the renderer: the expanded report shows the whole prompt.
  const shown = { task, goal: identity.goal ?? '', profile: identity.role ?? '' };
  entry.done = (async () => {
    try {
      const outcome = await launch(controller.signal);
      const note = await finish();
      // The outcome word the report starts with; a crashed run is failed whatever it said, and a report without the word is unknown.
      const reported = outcome.error ? 'failed' : reportStatus(outcome.text);
      if (reported) details.outcome = reported;
      const stats = [formatDuration(Date.now() - details.startedAt), toolCallCount(details.toolCalls), deniedLabel(details), compactionLabel(details), formatCost(details)].filter(Boolean).join(' · ');
      const head = outcome.error ? `Background subagent ${who} failed` : `Background subagent ${who} ${reported ? `reported ${reported}` : 'finished'} (${stats})`;
      const durationMs = Date.now() - details.startedAt;
      const summary = outcome.error ? `Failed: ${firstLine(outcome.error)}` : doneSummary(details, durationMs);
      const alreadyRead = read.consume(identity.id);
      const next = outcome.error ? `${FAILED_NEXT}${outcome.text ? PARTIAL_UNVERIFIED : ''}` : '';
      const notices = noticeLines(details).join('\n');
      // A successful run that lingers stays reachable: the report says for how long and how to use or end it.
      const reachable = outcome.error ? '' : (linger?.() ?? '');
      // The child's own notices sit with its words, inside the fence: only the parent's counts ride in the footer.
      const text = [outcome.text || '(no text)', notices].filter(Boolean).join('\n\n');
      const body = outcome.error ? `Error: ${outcome.error}\n${text}` : text;
      const shot = details.shot ? `(Last screenshot: ${shortPath(details.shot)})` : '';
      reports.add({ content: reportMessage(head, body, [next, ...runFooter(details, Boolean(outcome.error)), shot, note, reachable]), details: { id: identity.id, status: outcome.error ? 'failed' : 'done', ...(reported ? { outcome: reported } : {}), summary, durationMs, ...shown, ...reportFacts(details) }, wake: !alreadyRead });
    } catch (error) {
      const note = await finish();
      read.consume(identity.id);
      const by = entry.stoppedBy;
      const reason = by === USER_SENDER ? 'cancelled by the user' : by ? `stopped by ${by}` : `stopped: ${errorMessage(error)}`;
      const reachable = by === PERMISSION_STOP ? (linger?.() ?? '') : '';
      reports.add({
        // A stop someone asked for is the parent's own word; any other cause (a crash, a stall) quotes the child's output.
        content: by ? `Background subagent ${who} ${reason}${note}${reachable}` : reportMessage(`Background subagent ${who} stopped`, [errorMessage(error), ...noticeLines(details)].join('\n\n'), [...runFooter(details, true), note, reachable]),
        details: { id: identity.id, status: 'stopped', ...(by ? { requested: true } : { outcome: 'failed' as const }), summary: capitalize(reason), durationMs: Date.now() - details.startedAt, ...shown, ...reportFacts(details) },
        wake: !by,
      });
    } finally {
      background.delete(identity.id);
      runs.changed();
    }
  })();
}

export function registerAgentTool(pi: ExtensionAPI, getProfiles: () => Map<string, AgentProfile>, team: Team, permission: Permission, getRefused?: () => ReadonlyMap<string, string>): AgentControl {
  pi.registerMessageRenderer(RESULT_TYPE, renderAgentMessage);
  // Per registration, not per module: a replaced session starts clean.
  const panel = new AgentsView(team);
  const runs: AgentRuns = {
    pi, team, background: new Map(), foreground: new Map(), reports: new ReportQueue(pi), read: new ReadReports(), running: new Map(), slots: new Slots(), session: 0,
    changed: () => panel.setBackground(runs.background.size),
  };
  const { background } = runs;
  pi.on('tool_result', async (event, ctx) => void (!event.isError && background.size > 0 && runs.read.observe(event.toolName, event.input, ctx.cwd)));
  // When this session asked the user: an answer the parent gives right after one may be the user's.
  const questions = new UserQuestions();
  pi.on('tool_execution_start', async (event) => void (event.toolName === 'askUser' && questions.begin(event.toolCallId)));
  pi.on('tool_execution_end', async (event) => void (event.toolName === 'askUser' && questions.end(event.toolCallId)));
  /** Runs `work` and, once it ends, records the run's session file and its questions to this session's agent. */
  const record = <T>(run: Run, details: RunDetails, session: ChildSession, work: Promise<T>): Promise<T> =>
    recordRun(details, session, () => (run.identity.parentId ? questions.mark(team.asks(run.identity.id, run.identity.parentId, details.startedAt)) : []), work);
  let launching = new AbortController();
  const cancelLaunches = () => {
    launching.abort();
    launching = new AbortController();
  };
  /** The caller's signal joined with the one a permission change or shutdown aborts. */
  const withLaunch = (signal?: AbortSignal) => (signal ? AbortSignal.any([signal, launching.signal]) : launching.signal);
  // Finished background runs still reachable (OCTOCODE_SUBAGENT_LINGER_SECONDS): a message resumes their saved conversation.
  const lingering = new Lingering(runs, permission, withLaunch, (run, ctx, { text, message }) => {
    const details = runDetails(run);
    const track = tracker(run.identity, details, modelLookup(ctx));
    const session = childSession(run.identity);
    const args = buildAgentArgs(run.profile, run.route.model, undefined, undefined, session);
    const host = relayHost(ctx);
    const launch = (signal: AbortSignal) => record(run, details, session, runSubagent(message, args, ctx.cwd, run.profile, run.identity, signal, track(undefined), undefined, undefined, host));
    startBackground(runs, run, details, text, launch, () => Promise.resolve('').finally(run.release), ctx.sessionManager.getSessionId(), lingering.window(run, ctx));
  });
  const late = new LateReports(runs);
  const stopRuns = async (by: string): Promise<void> => {
    // Wait (bounded) for stopped children to file their reports; a named stop files them without starting a turn.
    for (const id of runs.foreground.keys()) stopBackground(runs.foreground, id, by, 'subagent');
    const stopping = [...background].map(([id, entry]) => {
      stopBackground(background, id, by);
      return entry.done;
    });
    await settleWithin(Promise.all(stopping), SHUTDOWN_WAIT_MS);
    runs.reports.flush();
  };
  // A reload stops them without asking: their reports (now, or at the next start when late) say it was the reload.
  const stopAll = (reason?: string) => stopRuns(reason === 'reload' ? 'the /reload' : 'session shutdown');
  /** What this session offers a child: its permission state, its UI when a person can be asked, and its team's signature for the user's messages. */
  const relayHost = (ctx: Pick<ExtensionContext, 'hasUI' | 'ui'>): RelayHost => ({ permission, vouch: (id, secret) => team.vouch(id, secret), ...(ctx.hasUI ? { ui: ctx.ui } : {}) });
  permission.onRevoke(() => {
    cancelLaunches();
    void stopRuns(PERMISSION_STOP);
  });
  pi.on('agent_settled', async () => late.take());
  pi.on('session_shutdown', async (event, ctx) => {
    cancelLaunches();
    late.stop();
    // Usually done already: index.ts runs it before the team leaves.
    // Reports still waiting in the batch window are saved for the next start, not lost with this runtime.
    runs.reports.persist();
    await stopAll(event.reason);
    // A child still closing keeps its entry (and its slot) until it exits; its report waits for this session's next start.
    runs.session += 1;
    lingering.forgetAll();
    panel.stop(ctx);
  });
  // Worktrees left by a Pi process that died: their changes are saved to the agent's ref, then the worktree goes (kept when saving fails).
  pi.on('session_start', async (_event, ctx) => {
    panel.start(ctx);
    await late.start(ctx.sessionManager.getSessionId());
    // Off the start path: it lists and deletes directories.
    const root = scratchRoot(ctx.cwd);
    setImmediate(() => void sweepScratch(root));
    void pruneWorktrees(ctx.cwd)
      .then(describePrune, (error: unknown) => ({ text: `Could not prune orphaned subagent worktrees: ${errorMessage(error)}`, level: 'warning' as const }))
      .then((note) => {
        if (note && ctx.hasUI) notify(ctx.ui, note.text, note.level);
      })
      // The session may have been replaced meanwhile (/new, /resume): its ctx then throws, and nobody is left to tell.
      .catch(() => undefined);
  });
  pi.registerTool(timedTool({
    name: 'agent',
    label: 'Agent',
    description:
      'Delegate a self-contained task to a fresh Pi session that does not inherit this conversation. ' +
      `Foreground calls return the report; background calls return the agent's name and its report arrives later. At most ${maxSubagents()} run at once; a call past that waits for one to finish. ` +
      'A long report names its full file. Never poll or sleep for a background report. ' +
      'A subagent updates only backlog items you claimed: claim one first and name its id in the task.',
    promptSnippet: 'Delegate bounded work to a subagent',
    parameters: Type.Object({
      goal: Type.String({ description: GOAL_DESCRIPTION }),
      task: Type.String({ description: 'Task contract: objective; inputs (paths, evidence and decisions the subagent cannot infer); constraints (bounded scope, existing authorization, budget when costly); output; Definition of Done' }),
      profile: Type.Optional(Type.String({ description: 'Profile name from the active Profiles list; omit for a general-purpose subagent' })),
      model: Type.Optional(Type.String({ description: 'Model override: provider/id, bare id or purpose (small, medium), optionally :thinking. Refused when unavailable; omit for the profile\'s model, else this session\'s' })),
      background: Type.Optional(Type.Boolean({ description: 'Return at once for long or parallel work. `sendMessage` redirects it; `coordinate stop` ends it' })),
      collaborate: Type.Optional(Type.Boolean({ description: 'true on each call of subagents that must coordinate directly (shared contracts; name each one\'s owner in the tasks, RACI); false for independent tasks' })),
      isolate: Type.Optional(Type.Boolean({ description: 'Work in a private git worktree; changes land on refs/octocode/pi/<id>, not in this tree. The report names the ref: ask the user to run `/agents merge <id>` to bring them in' })),
    }),
    async execute(id, params, signal, onUpdate, ctx) {
      const launchSignal = withLaunch(signal);
      launchSignal.throwIfAborted();
      const goal = checkGoal(params.goal);
      // Refusals first (unknown profile, unavailable model), then a slot, then the spawn ask: the user is never asked
      // for a run that cannot start, and a denied spawn claims and starts nothing.
      const plan = planRun(getProfiles(), params, ctx, getRefused?.());
      await runs.slots.take(launchSignal, (running) => onUpdate?.(waitingUpdate(params.profile, goal, running)));
      let claimed: Run | undefined;
      let grants: Awaited<ReturnType<typeof askSpawn>>;
      try {
        grants = await askSpawn(permission, ctx, plan.profile, params.task, launchSignal, pi.getSettings().defaultTools, { toolCallId: id, goal });
        // An isolated run's name must not repeat a saved ref: `/agents merge <name>` would be ambiguous.
        const refs = params.isolate ? await pendingRefs(ctx.cwd) : [];
        launchSignal.throwIfAborted();
        claimed = claimRun(runs, plan, { ...params, goal }, ctx, refs);
      } finally {
        // From here the run's `release` gives the slot back.
        if (!claimed) runs.slots.give();
      }
      const run = claimed;
      // A child cannot ask anyone: it also gets every kind the user allowed for this session.
      run.identity.permission = { mode: permission.mode, grants: [...new Set([...grants, ...permission.childGrants()])] };
      // With a person to ask, the child's approval dialogs come here (this tool exists only in the main session).
      run.identity.relay = ctx.hasUI;
      const host = relayHost(ctx);
      const isolation = new Isolation(run.identity.id);
      let finalized: Promise<string> | undefined;
      const finish = () => (finalized ??= isolation.finish().finally(run.release));
      let handedOff = false;
      try {
        const cwd = params.isolate ? await isolation.start(ctx, run.identity) : ctx.cwd;
        launchSignal.throwIfAborted();
        const worktree = isolation.worktree;
        const warning = `${worktree?.warning ? `\n\n${worktree.warning}` : ''}${run.teamNote}`;
        const onSpawn = worktree ? (pid: number) => recordWorktreePid(worktree, pid) : undefined;
        const library = existingLibrary();
        const own = withIdentity(params.task, run.identity, library ? { library } : undefined);
        const task = run.identity.collaborate ? joinTeam(runs, run, own) : own;
        const details = runDetails(run);
        const track = tracker(run.identity, details, modelLookup(ctx));
        const session = childSession(run.identity);
        // Only a background run that is not isolated (its worktree is gone) can stay reachable for a follow-up.
        const lingers = params.background === true && !params.isolate && lingerSeconds() > 0;
        const args = buildAgentArgs(run.profile, run.route.model, undefined, undefined, session);
        const launch = (runSignal: AbortSignal | undefined, update: AgentUpdate | undefined) =>
          record(run, details, session, runSubagent(task, args, cwd, run.profile, run.identity, runSignal, track(update), onSpawn, undefined, host));
        const stopping = () => {
          details.stopping = true;
          onUpdate?.({ content: [{ type: 'text', text: 'stopping…' }], details: { ...details } });
        };
        if (!params.background) return await runForeground(runs, run, details, { launch: (runSignal) => launch(runSignal, onUpdate), finish, warning, signal: launchSignal, stopping });
        startBackground(runs, run, details, params.task, (runSignal) => launch(runSignal, undefined), finish, ctx.sessionManager.getSessionId(), lingers ? lingering.window(run, ctx) : undefined);
        handedOff = true;
        details.status = 'background';
        return { content: [{ type: 'text', text: `Started ${run.identity.id} (${run.identity.role}: ${goal}) in the background; its report arrives as a message.${warning}` }], details };
      } finally {
        // A failure before the run started still removes the worktree.
        if (!handedOff) await finish();
      }
    },
    renderCall: renderAgentCall,
    renderResult: renderAgentResult,
  }));
  team.setResumer((id, text, from, signal) => lingering.resume(id, text, from, signal));
  return {
    // A lingering run has nothing running: stopping it ends its window.
    stop: (id, by) => (lingering.forget(id) ? undefined : stopBackground(background, id, by)),
    // Reports still inside their batching window go out now: a held headless run hands them to its next turn.
    settle: () => Promise.all([...background.values()].map((entry) => entry.done)).then(() => runs.reports.flush()),
    background: () => background.size,
    stopAll,
    command: agentsCommand(team, background, runs.foreground, questions),
  };
}
