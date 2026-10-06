import type { Usage } from '@earendil-works/pi-ai';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';
import { envFlag, envInt } from '../shared/env.js';
import { formatDuration, shortPath, toolCallCount } from '../shared/format.js';
import { errorMessage, firstLine, settleWithin } from '../shared/util.js';
import { AgentsView } from '../team/panel.js';
import { newId } from '../team/routing.js';
import { USER_SENDER, type Team } from '../team/session.js';
import { timedTool } from '../shared/render.js';
import { projectTrustNow } from '../shared/trust.js';
import { doneSummary, renderAgentCall, renderAgentMessage, renderAgentResult, type RunDetails } from './render.js';
import { buildAgentArgs, runSubagent, type Identity } from './process.js';
import type { AgentProfile } from './profiles.js';
import { agentScratch, dropEmptyScratch, ReadReports, ReportQueue, RESULT_TYPE, scratchRoot, sweepScratch } from './handoff.js';
import { saveShot, shotsDir, type Shot } from './screenshots.js';
import { agentsCommand, stopBackground, type Background, type BackgroundRun } from './command.js';
import type { Subcommand } from '../shared/commands.js';
import { createWorktree, describePrune, finishWorktree, isolationReport, pruneWorktrees, recordWorktreePid, teamWorkspace, type Worktree } from './worktree.js';




/** What the rest of the extension may do with the subagents this session started. */
export interface AgentControl {
  /** Stops a background subagent this session started; the refusal text when there is none by that id. */
  stop(id: string, by: string): string | undefined;
  /** Resolves once every background subagent has finished and sent its report. */
  settle(): Promise<unknown>;
  /** Background subagents running now (a session switch would stop them). */
  background(): number;
  /** `/octocode agents`: list agents, tell, kill, merge; `index.ts` registers it and its `/agents` shortcut. */
  command: Subcommand;
}

/** Longest accepted task; a runaway task text is refused by schema validation instead of spawning a child. */
export const MAX_TASK_CHARS = 16_384;

const COLLABORATE_ENV = 'OCTOCODE_SUBAGENT_COLLABORATE';

/** Whether subagents collaborate when a call does not say: `OCTOCODE_SUBAGENT_COLLABORATE` = 1/true/on, else no. */
export function collaborateByDefault(env: NodeJS.ProcessEnv = process.env): boolean {
  return envFlag(env, COLLABORATE_ENV);
}

/** The task a collaborating subagent receives: its own task plus who else is on the team and how to reach them. */
export function teamTask(task: string, parentId: string, others: Array<[string, string]>): string {
  const list = others.length > 0 ? others.map(([id, line]) => `- \`${id}\`: ${line}`).join('\n') : '- none yet; `coordinate list` shows teammates that start later.';
  return `${task}\n\n## Teammates\nYou collaborate with the other subagents that ${parentId} started for the same goal. Running now:\n${list}`;
}

/** How long shutdown waits for stopped background subagents: past the SIGTERM grace, after which they are killed. */
const SHUTDOWN_WAIT_MS = 6_000;

export const MAX_SUBAGENTS_ENV = 'OCTOCODE_MAX_SUBAGENTS';
const DEFAULT_MAX_SUBAGENTS = 3;

/** How many subagents may run at once: `OCTOCODE_MAX_SUBAGENTS` (a positive integer), else 3. Many parallel children get messy. */
export function maxSubagents(env: NodeJS.ProcessEnv = process.env): number {
  const value = envInt(env, MAX_SUBAGENTS_ENV, DEFAULT_MAX_SUBAGENTS);
  return value >= 1 ? value : DEFAULT_MAX_SUBAGENTS;
}

/** The refusal for a call over the limit, or undefined when there is room. */
export function subagentLimitError(active: Iterable<string>, limit: number): string | undefined {
  const ids = [...active];
  if (ids.length < limit) return undefined;
  return `${ids.length} subagents are already running (${ids.join(', ')}); the limit is ${limit}. Wait for one to finish (background answers arrive as messages), pass the work to a running subagent with \`sendMessage\`, or do it yourself. The limit is set by ${MAX_SUBAGENTS_ENV}.`;
}

/** State of one agent tool registration; a replaced session starts clean. */
interface AgentRuns {
  pi: ExtensionAPI;
  team: Team;
  background: Background;
  /** Foreground runs, so `/agents kill` can stop one without aborting the whole turn. */
  foreground: Background;
  reports: ReportQueue;
  read: ReadReports;
  /** Subagents running now (foreground and background), by id. */
  running: Map<string, string | undefined>; // A task line is present once a collaborating run launches.
}

interface AgentParams {
  task: string;
  profile?: string;
  model?: string;
  background?: boolean;
  collaborate?: boolean;
  isolate?: boolean;
}

type Outcome = Awaited<ReturnType<typeof runSubagent>>;
type AgentUpdate = (update: { content: Array<{ type: 'text'; text: string } | { type: 'image'; data: string; mimeType: string }>; details: RunDetails }) => void;

/** A claimed subagent run: who it is, who started it, where it hands files back, and how it gives its slot back. */
interface Run {
  identity: Identity;
  profile: AgentProfile | undefined;
  /** Why the run starts without the team (its database is unusable), for the result. */
  teamNote: string;
  release: () => void;
}

/**
 * Checks the profile and the subagent limit, then claims a slot. Checked and claimed with no await in between, so
 * parallel calls in one turn cannot overshoot the limit.
 */
function claimRun(runs: AgentRuns, profiles: Map<string, AgentProfile>, params: AgentParams, cwd: string): Run {
  const profile = params.profile ? profiles.get(params.profile) : undefined;
  if (params.profile && !profile) throw new Error(`Unknown profile "${params.profile}". Available: ${[...profiles.keys()].join(', ')}`);
  const overLimit = subagentLimitError(runs.running.keys(), maxSubagents());
  if (overLimit) throw new Error(overLimit);
  // Joining gives the child its parent's address for sendMessage; without a usable team database it still runs, alone.
  let parentId: string | undefined;
  let teamNote = '';
  try {
    parentId = runs.team.join().id;
  } catch (error) {
    teamNote = `\n\n(Started without the team: ${errorMessage(error)}. It cannot message you or its siblings.)`;
  }
  const collaborate = parentId !== undefined && (params.collaborate ?? collaborateByDefault());
  const identity: Identity = { id: newId(profile?.name ?? 'general'), task: firstLine(params.task).slice(0, 160), ...(parentId ? { parentId } : {}), ...(collaborate ? { collaborate } : {}) };
  runs.running.set(identity.id, undefined);
  const scratch = agentScratch(cwd, identity.id);
  if (scratch) identity.scratch = scratch;
  const release = () => {
    if (!runs.running.delete(identity.id)) return;
    const kept = scratch && !dropEmptyScratch(scratch);
    runs.team.rememberFinished(identity.id, `its report (or failure) already reached you${kept ? ` (files it handed back, and its full report when cut, are in ${shortPath(scratch)})` : ''}.`);
  };
  return { identity, profile, teamNote, release };
}

/** A collaborating run's task, listing its running teammates (they find the newcomer with `coordinate list`). */
function joinTeam(runs: AgentRuns, run: Run, task: string): string {
  const others = [...runs.running].filter((entry): entry is [string, string] => entry[1] !== undefined);
  const withTeam = teamTask(task, run.identity.parentId ?? 'your parent', others);
  runs.running.set(run.identity.id, run.identity.task ?? '');
  return withTeam;
}

/** An isolated run's private worktree; `finish` commits its changes to the agent's ref and removes it, once, and returns the text for the report. */
class Isolation {
  worktree: Worktree | undefined;

  constructor(private readonly id: string) {}

  /** Creates the worktree and returns the child's cwd. The worktree is its own git root: the child stays on this team. */
  async start(ctx: Pick<ExtensionContext, 'cwd' | 'isProjectTrusted'>, identity: Identity): Promise<string> {
    this.worktree = await createWorktree(ctx.cwd, this.id);
    identity.workspace = teamWorkspace(ctx.cwd);
    // The worktree is a different root, so no stored decision matches it: hand down only a decision this session made.
    if (projectTrustNow(ctx) === true) identity.trustRoot = identity.workspace;
    return this.worktree.cwd;
  }

  finish(): Promise<string> {
    const worktree = this.worktree;
    return worktree
      ? finishWorktree(worktree).then(
          (result) => isolationReport(this.id, result),
          (error: unknown) => isolationReport(this.id, { error: errorMessage(error) }),
        )
      : Promise.resolve('');
  }
}

/** Progress of a run into `details` (tool lines, usage, the latest screenshot), mirrored to `onUpdate`. */
function tracker(identity: Identity, details: RunDetails) {
  const dir = shotsDir();
  let shots = 0;
  let latest: Shot | undefined;
  return (onUpdate: AgentUpdate | undefined) => (line: string | undefined, usage: Usage, image?: { data: string; mimeType: string }) => {
    if (line) {
      details.toolCalls += 1;
      details.activity = [...details.activity.slice(-4), line];
    }
    if (image) {
      latest = saveShot(dir, identity.id, (shots += 1), image);
      details.shot = latest.path;
    }
    // Cached prompt tokens are still prompt tokens: without them a cached run shows ↑3.
    details.input = usage.input + usage.cacheRead + usage.cacheWrite;
    details.output = usage.output;
    const text = { type: 'text' as const, text: line ?? details.activity.at(-1) ?? 'working' };
    // Partial results only reach the user's screen, so the latest screenshot is drawn there but never enters the model's context.
    onUpdate?.({ content: image && latest?.data ? [text, { type: 'image', data: latest.data, mimeType: latest.mimeType }] : [text], details: { ...details } });
  };
}

const withShot = (details: RunDetails, text: string) => (details.shot ? `${text}\n\n(Last screenshot: ${shortPath(details.shot)})` : text);

/** Runs the subagent in the background; its report (or why it stopped) arrives as a message, and then it gives its slot back. */
function startBackground(runs: AgentRuns, run: Run, details: RunDetails, task: string, launch: (signal: AbortSignal) => Promise<Outcome>, finish: () => Promise<string>): void {
  const { background, reports, read } = runs;
  const { identity } = run;
  const controller = new AbortController();
  const entry: BackgroundRun = { controller, done: Promise.resolve() };
  background.set(identity.id, entry);
  read.track(identity.id, identity.scratch);
  // The parent may have many runs going: every report names the task it answers.
  const taskLine = identity.task ? `\nTask: ${identity.task}` : '';
  // For the renderer: the expanded report shows the whole prompt.
  const shown = { task };
  entry.done = (async () => {
    try {
      const outcome = await launch(controller.signal);
      const note = await finish();
      details.seconds = Math.round((Date.now() - details.startedAt) / 1000);
      const cost = outcome.usage.cost.total > 0 ? ` · $${outcome.usage.cost.total.toFixed(2)}` : '';
      const head = outcome.error ? `Background subagent ${identity.id} failed: ${outcome.error}${taskLine}` : `Background subagent ${identity.id} finished (${formatDuration(Date.now() - details.startedAt)} · ${toolCallCount(details.toolCalls)}${cost}):${taskLine}`;
      const durationMs = Date.now() - details.startedAt;
      const summary = outcome.error ? `Failed: ${firstLine(outcome.error)}` : doneSummary(details, durationMs);
      const alreadyRead = read.consume(identity.id);
      reports.add({ content: `${head}\n${withShot(details, outcome.text || '(no text)')}${note}`, details: { id: identity.id, status: outcome.error ? 'failed' : 'done', summary, durationMs, ...shown }, wake: !alreadyRead });
    } catch (error) {
      const note = await finish();
      read.consume(identity.id);
      const by = entry.stoppedBy;
      const reason = by === USER_SENDER ? 'cancelled by the user' : by ? `stopped by ${by}` : `stopped: ${errorMessage(error)}`;
      reports.add({
        content: `Background subagent ${identity.id} ${reason}${taskLine}${note}`,
        details: { id: identity.id, status: 'stopped', summary: reason.charAt(0).toUpperCase() + reason.slice(1), durationMs: Date.now() - details.startedAt, ...shown, ...(by ? { requested: true } : {}) },
        wake: !by,
      });
    } finally {
      background.delete(identity.id);
    }
  })();
}

/** Runs the subagent and waits for its report; the child's model usage counts toward this session's totals. */
async function runForeground(
  runs: AgentRuns, run: Run, details: RunDetails, launch: (signal: AbortSignal) => Promise<Outcome>, finish: () => Promise<string>, warning: string, signal: AbortSignal | undefined,
) {
  const { identity } = run;
  // Its own controller, so `/agents kill` can stop this run without aborting the parent's turn.
  const controller = new AbortController();
  const entry: BackgroundRun = { controller, done: Promise.resolve() };
  const abort = () => controller.abort();
  if (signal?.aborted) controller.abort();
  signal?.addEventListener('abort', abort, { once: true });
  runs.foreground.set(identity.id, entry);
  let outcome: Outcome;
  try {
    outcome = await launch(controller.signal);
  } catch (error) {
    const note = await finish();
    const reason = entry.stoppedBy === USER_SENDER ? new Error(`Subagent ${identity.id} was stopped by the user.`) : error;
    throw note ? new Error(`${errorMessage(reason)}${note}`) : reason;
  } finally {
    signal?.removeEventListener('abort', abort);
    runs.foreground.delete(identity.id);
  }
  const note = `${await finish()}${warning}`;
  details.seconds = Math.round((Date.now() - details.startedAt) / 1000);
  if (outcome.error) {
    // An error result rather than a throw, so the child's spent tokens still reach this session's totals.
    details.status = 'failed';
    const text = `Subagent ${identity.id} failed: ${outcome.error}${outcome.text ? `\n\nPartial answer:\n${outcome.text}` : ''}${note}`;
    return { isError: true, content: [{ type: 'text' as const, text }], details, usage: outcome.usage };
  }
  details.status = 'done';
  return { content: [{ type: 'text' as const, text: `${withShot(details, outcome.text || '(subagent returned no text)')}${note}` }], details, usage: outcome.usage };
}

export function registerAgentTool(pi: ExtensionAPI, getProfiles: () => Map<string, AgentProfile>, team: Team): AgentControl {
  pi.registerMessageRenderer(RESULT_TYPE, renderAgentMessage);
  // Per registration, not per module: a replaced session starts clean.
  const runs: AgentRuns = { pi, team, background: new Map(), foreground: new Map(), reports: new ReportQueue(pi), read: new ReadReports(), running: new Map() };
  const { background } = runs;
  pi.on('tool_result', async (event, ctx) => {
    if (!event.isError && background.size > 0) runs.read.observe(event.toolName, event.input, ctx.cwd);
    return undefined;
  });
  const panel = new AgentsView(team);
  pi.on('session_shutdown', async (_event, ctx) => {
    // Wait (bounded) for stopped children to file their reports; a named stop files them without starting a turn.
    const stopping = [...background].map(([id, entry]) => (stopBackground(background, id, 'session shutdown'), entry.done));
    await settleWithin(Promise.all(stopping), SHUTDOWN_WAIT_MS);
    runs.reports.flush();
    background.clear();
    runs.running.clear();
    panel.stop(ctx);
  });
  // Worktrees left by a Pi process that died: their changes are saved to the agent's ref, then the worktree goes (kept when saving fails).
  pi.on('session_start', async (_event, ctx) => {
    panel.start(ctx);
    // Off the start path: it lists and deletes directories.
    const root = scratchRoot(ctx.cwd);
    setImmediate(() => sweepScratch(root));
    void pruneWorktrees(ctx.cwd)
      .then(describePrune, (error: unknown) => ({ text: `Could not prune orphaned subagent worktrees: ${errorMessage(error)}`, level: 'warning' as const }))
      .then((note) => {
        if (note && ctx.hasUI) ctx.ui.notify(note.text, note.level);
      });
  });
  pi.registerTool(timedTool({
    name: 'agent',
    label: 'Agent',
    description:
      'Delegate a self-contained task to a fresh Pi session. It does not inherit this conversation: include the goal, scope, paths, relevant evidence, accepted decisions and expected result. Choose a profile by its task and capabilities. ' +
      `Foreground calls return the report; background calls return an agent id and deliver the report automatically later. Independent calls can run in parallel, at most ${maxSubagents()} at once (background included); extra calls are refused. ` +
      'Reports include verification and blockers; long reports name their full file. Review the result before integrating it.',
    promptSnippet: 'Delegate a self-contained task to a subagent',
    parameters: Type.Object({
      task: Type.String({ description: 'Goal, bounded scope, paths, context the child cannot infer, existing authorization and required output; no conversation is inherited', maxLength: MAX_TASK_CHARS, minLength: 1 }),
      profile: Type.Optional(Type.String({ description: 'Profile name from the active Profiles list; omit for a general worker. Profiles set instructions, tool exclusions and MCP access' })),
      model: Type.Optional(Type.String({ description: 'Model override (provider/id)' })),
      background: Type.Optional(Type.Boolean({ description: 'Return at once for long or parallel work; the report arrives as a message. `sendMessage` redirects it; `coordinate stop` ends it' })),
      collaborate: Type.Optional(Type.Boolean({ description: 'Give sibling workers on one goal teammate context for direct coordination; useful for shared contracts, unnecessary for unrelated tasks' })),
      isolate: Type.Optional(Type.Boolean({ description: 'Work in a private git worktree; changes land on refs/octocode/pi/<id>, not in this tree' })),
    }),
    async execute(_id, params, signal, onUpdate, ctx) {
      const run = claimRun(runs, getProfiles(), params, ctx.cwd);
      const isolation = new Isolation(run.identity.id);
      let finalized: Promise<string> | undefined;
      const finish = () => (finalized ??= isolation.finish().finally(run.release));
      let handedOff = false;
      try {
        const cwd = params.isolate ? await isolation.start(ctx, run.identity) : ctx.cwd;
        const worktree = isolation.worktree;
        const warning = `${worktree?.warning ? `\n\n${worktree.warning}` : ''}${run.teamNote}`;
        const onSpawn = worktree ? (pid: number) => recordWorktreePid(worktree, pid) : undefined;
        const task = run.identity.collaborate ? joinTeam(runs, run, params.task) : params.task;
        const details: RunDetails = { id: run.identity.id, profile: run.profile?.name ?? 'general', toolCalls: 0, activity: [], input: 0, output: 0, startedAt: Date.now(), status: 'running' };
        const track = tracker(run.identity, details);
        const args = buildAgentArgs(task, run.profile, params.model);
        const launch = (runSignal: AbortSignal | undefined, update: AgentUpdate | undefined) => runSubagent(args, cwd, run.profile, run.identity, runSignal, track(update), onSpawn);
        if (!params.background) return await runForeground(runs, run, details, (runSignal) => launch(runSignal, onUpdate), finish, warning, signal);
        startBackground(runs, run, details, params.task, (runSignal) => launch(runSignal, undefined), finish);
        handedOff = true;
        details.status = 'background';
        return { content: [{ type: 'text', text: `Started ${run.identity.id} in the background; its report arrives as a message.${warning}` }], details };
      } finally {
        if (!handedOff) {
          // A failure before the run started still removes the worktree.
          await finish();
        }
      }
    },
    renderCall: renderAgentCall,
    renderResult: renderAgentResult,
  }));
  return {
    stop: (id, by) => stopBackground(background, id, by),
    // Reports still inside their batching window go out now: a held headless run hands them to its next turn.
    settle: () => Promise.all([...background.values()].map((entry) => entry.done)).then(() => runs.reports.flush()),
    background: () => background.size,
    command: agentsCommand(team, background, runs.foreground),
  };
}
