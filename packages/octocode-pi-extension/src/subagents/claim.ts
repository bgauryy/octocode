import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import path from 'node:path';
import { archiveSearch, LIBRARY_FILE } from '../shared/archive.js';
import { envInt } from '../shared/env.js';
import { shortPath } from '../shared/format.js';
import { sessionFolderNames } from '../shared/home.js';
import { errorMessage } from '../shared/util.js';
import type { Team } from '../team/session.js';
import { currentCatalog } from './catalog.js';
import { routeModel, type ModelRoute } from './model.js';
import type { Identity } from './launch.js';
import { GENERAL_PROFILE, type AgentProfile } from './profiles.js';
import { agentScratch, dropEmptyScratch, scratchNames, type ReadReports, type ReportQueue } from './handoff.js';
import type { Background } from './command.js';

/** Starting a subagent: who it is (name, role, goal), its slot under the limit, and the task text it receives. */

/** The task a collaborating subagent receives: its own task plus who else is on the team (`name`: `role: goal`) and how to reach them. */
export function teamTask(task: string, parentId: string, others: Array<[string, string]>): string {
  const list = others.length > 0 ? others.map(([id, line]) => `- \`${id}\`: ${line}`).join('\n') : '- none yet; `coordinate list` shows teammates that start later.';
  return `${task}\n\n## Teammates\nYou collaborate with the other subagents that ${parentId} started for the same goal. Running now:\n${list}`;
}

/**
 * The facts only this child has. They ride in its task, not its system prompt, so every child of one profile shares a
 * cached prompt prefix. `archive` is the library of the parent's compacted history (`shared/archive.ts`), given only
 * when the parent has one; the child reads it with the `archive` tool, nothing may change it.
 */
export function withIdentity(task: string, identity: Pick<Identity, 'id' | 'role' | 'goal' | 'parentId' | 'scratch'>, archive?: { library: string }): string {
  const parent = identity.parentId ? `\`${identity.parentId}\`` : 'another agent';
  const scratch = identity.scratch ? `\`${shortPath(identity.scratch)}\`` : '`.octocode/tmp/` in the repository';
  const role = identity.role ? ` (${identity.role})` : '';
  const goal = identity.goal ? ` Your goal: ${identity.goal}.` : '';
  return `${task}\n\n## You\nYour name is \`${identity.id}\`${role}.${goal} You work for ${parent}. Your scratch folder, for handoff files and a long report, is ${scratch}.${archive ? `\nEarlier history of your parent session, compacted out of its context, is archived in ${path.dirname(archive.library)} (read-only; ${LIBRARY_FILE} indexes it; the repository shows today's state, the archive what was said and found then). ${archiveSearch()}` : ''}`;
}

export const MAX_SUBAGENTS_ENV = 'OCTOCODE_MAX_SUBAGENTS';
const DEFAULT_MAX_SUBAGENTS = 3;

/** How many subagents may run at once: `OCTOCODE_MAX_SUBAGENTS` (a positive integer), else 3. A call past it waits for a slot. */
export function maxSubagents(env: NodeJS.ProcessEnv = process.env): number {
  const value = envInt(env, MAX_SUBAGENTS_ENV, DEFAULT_MAX_SUBAGENTS);
  return value >= 1 ? value : DEFAULT_MAX_SUBAGENTS;
}

const SUBAGENT_LINGER_ENV = 'OCTOCODE_SUBAGENT_LINGER_SECONDS';

/** Seconds a finished background subagent stays reachable (a message resumes its saved conversation); 0 = off. */
export function lingerSeconds(env: NodeJS.ProcessEnv = process.env): number {
  return Math.max(0, envInt(env, SUBAGENT_LINGER_ENV, 0));
}

/** The refusal for a resume while every slot is taken (a message is answered now; it does not wait for a slot). */
export function subagentLimitError(active: Iterable<string>, limit: number): string {
  const ids = [...active];
  return `${ids.length} subagents are already running${ids.length ? ` (${ids.join(', ')})` : ''}; the limit is ${limit}. Wait for one to finish (background answers arrive as messages), pass the work to a running subagent with \`sendMessage\`, or do it yourself. The limit is set by ${MAX_SUBAGENTS_ENV}.`;
}

/**
 * Subagent slots under `maxSubagents()`. A call past the limit waits, first come first served, until a run gives its
 * slot back; its signal (Esc, a turn abort) gives up the wait. A slot given back passes straight to the next waiter.
 */
export class Slots {
  private taken = 0;
  private readonly waiting: Array<() => void> = [];

  constructor(private readonly limit: () => number = maxSubagents) {}

  /** Takes a free slot without waiting; false when all are in use or someone already waits. */
  tryTake(): boolean {
    if (this.waiting.length > 0 || this.taken >= this.limit()) return false;
    this.taken += 1;
    return true;
  }

  /** Takes a slot, waiting while all are in use; `onWait` hears how many run when the wait begins. */
  async take(signal: AbortSignal | undefined, onWait?: (running: number) => void): Promise<void> {
    signal?.throwIfAborted();
    if (this.tryTake()) return;
    onWait?.(this.taken);
    await new Promise<void>((resolve, reject) => {
      const grant = () => {
        signal?.removeEventListener('abort', cancel);
        resolve();
      };
      const cancel = () => {
        const index = this.waiting.indexOf(grant);
        if (index >= 0) this.waiting.splice(index, 1);
        reject(new Error('Cancelled while waiting for a subagent slot.'));
      };
      this.waiting.push(grant);
      signal?.addEventListener('abort', cancel, { once: true });
    });
  }

  give(): void {
    const next = this.waiting.shift();
    if (next) next();
    else this.taken = Math.max(0, this.taken - 1);
  }
}

/** State of one agent tool registration; a replaced session starts clean. */
export interface AgentRuns {
  pi: ExtensionAPI;
  team: Team;
  background: Background;
  /** Foreground runs, so `/agents kill` can stop one without aborting the whole turn. */
  foreground: Background;
  reports: ReportQueue;
  read: ReadReports;
  /** Subagents running now (foreground and background), by name. */
  running: Map<string, string | undefined>; // A `role: goal` line is present once a collaborating run launches.
  /** Their slots under the limit, taken before the spawn ask and given back by `Run.release`. */
  slots: Slots;
  /** Called when a background run starts or ends: the footer counts them while the agents panel cannot show them. */
  changed: () => void;
  /** Bumped when a session ends: a background run started under an earlier session drops its late report. */
  session: number;
}

/** What a call asks for: its goal (already checked by `checkGoal`), a profile and model to plan with, and whether its subagents work together. */
interface AgentParams {
  goal: string;
  profile?: string;
  model?: string;
  collaborate?: boolean;
}

/** A claimed subagent run: who it is, who started it, where it hands files back, and how it gives its slot back. */
export interface Run {
  identity: Identity & { role: string; goal: string };
  profile: AgentProfile | undefined;
  /** The model it runs on (the parent's when nothing else routes); `fallback` is the profile's unavailable preference. */
  route: Extract<ModelRoute, { model?: string }>;
  /** Why the run starts without the team (its database is unusable), for the result. */
  teamNote: string;
  release: () => void;
}

/** What a call asks for, checked before anything waits, asks or starts: its profile and the model it routes to. */
type RunPlan = Pick<Run, 'profile' | 'route'>;

/**
 * Checks the profile and routes the model; throws the refusal (unknown profile, unavailable model). `refused` holds why
 * each profile file was refused when the profiles loaded, by file name: an unknown name that is one of them says why.
 */
export function planRun(
  profiles: Map<string, AgentProfile>,
  params: Pick<AgentParams, 'profile' | 'model'>,
  ctx: Pick<ExtensionContext, 'modelRegistry' | 'model'>,
  refused: ReadonlyMap<string, string> = new Map(),
): RunPlan {
  const profile = params.profile ? profiles.get(params.profile) : undefined;
  if (params.profile && !profile) {
    const why = refused.get(params.profile);
    throw new Error(`Unknown profile "${params.profile}". Available: ${[...profiles.keys()].join(', ')}${why ? `\n"${params.profile}" was refused when profiles loaded: ${why}` : ''}`);
  }
  // An explicit model is strict; a profile's is a preference that falls back to this session's model.
  const route = routeModel({ explicit: params.model, preferred: profile?.model, available: ctx.modelRegistry?.getAvailable() ?? [], parent: ctx.model, catalog: currentCatalog() });
  if ('error' in route) throw new Error(route.error);
  return { profile, route };
}

/**
 * Names the run and claims it, in the slot the caller already took (`runs.slots`); `release` gives the slot back.
 * `avoid`: names to skip besides the team's, those of existing scratch folders and those of existing session folders
 * (a child's session id is its name, and its archive is global and outlives scratch), e.g. existing refs of isolated runs. A
 * resumed run keeps its own name, and with it its saved conversation; a new one never gets a name with a saved one.
 */
export function claimRun(runs: AgentRuns, plan: RunPlan, params: Pick<AgentParams, 'goal' | 'collaborate'>, ctx: Pick<ExtensionContext, 'cwd' | 'sessionManager'>, avoid: Iterable<string> = [], resumed?: Run['identity']): Run {
  const { cwd } = ctx;
  const { profile, route } = plan;
  // Joining gives the child its parent's address for sendMessage; without a usable team database it still runs, alone.
  let parentId: string | undefined;
  let teamNote = '';
  try {
    parentId = runs.team.join().id;
  } catch (error) {
    teamNote = `\n\n(Started without the team: ${errorMessage(error)}. It cannot message you or its teammates.)`;
  }
  const collaborate = parentId !== undefined && params.collaborate === true;
  // A resumed run continues as the same agent: name, role, goal, scratch folder and permission.
  const identity: Run['identity'] = resumed ?? {
    id: runs.team.newName([...runs.running.keys(), ...scratchNames(cwd), ...sessionFolderNames(), ...avoid]),
    role: profile?.name ?? GENERAL_PROFILE,
    goal: params.goal,
    parentSession: ctx.sessionManager.getSessionId(),
    ...(parentId ? { parentId } : {}),
    ...(collaborate ? { collaborate } : {}),
  };
  runs.running.set(identity.id, undefined);
  const scratch = agentScratch(cwd, identity.id);
  if (scratch) identity.scratch = scratch;
  const release = () => {
    if (!runs.running.delete(identity.id)) return;
    runs.slots.give();
    const kept = scratch && !dropEmptyScratch(scratch);
    runs.team.rememberFinished(identity.id, `its report (or failure) already reached you${kept ? ` (files it handed back, and its full report when cut, are in ${shortPath(scratch)})` : ''}.`);
  };
  return { identity, profile, route, teamNote, release };
}

/** A collaborating run's task, listing its running teammates (they find the newcomer with `coordinate list`). */
export function joinTeam(runs: AgentRuns, run: Run, task: string): string {
  const others = [...runs.running].filter((entry): entry is [string, string] => entry[1] !== undefined);
  const withTeam = teamTask(task, run.identity.parentId ?? 'your parent', others);
  runs.running.set(run.identity.id, `${run.identity.role}: ${run.identity.goal}`);
  return withTeam;
}
