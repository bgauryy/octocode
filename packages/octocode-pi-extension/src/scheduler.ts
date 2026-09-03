import { execFile } from 'node:child_process';
import { buildAwarenessCommand, runAwarenessInProcess } from './assets.js';
import type { PiContext, PiExecResult, PiInstance } from './types.js';

const DEFAULT_JOB_TIMEOUT_MS = 60_000;
const DEFAULT_CRON_JOB_NAME = 'awareness-status';
export const DEFAULT_AWARENESS_STATUS_INTERVAL_MS = 30 * 60 * 1000;

export type OctocodeCronJobStatus =
  | 'idle'
  | 'scheduled'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'skipped'
  | 'cancelled';

export interface OctocodeCronJobDefinition {
  name: string;
  label: string;
  description: string;
  intervalMs: number;
  enabledByDefault: boolean;
  awarenessArgs(ctx: PiContext | undefined): string[];
}

export interface OctocodeCronJobSnapshot {
  name: string;
  label: string;
  description: string;
  intervalMs: number;
  enabled: boolean;
  status: OctocodeCronJobStatus;
  running: boolean;
  nextRunAt?: string;
  lastStartedAt?: string;
  lastFinishedAt?: string;
  lastExitCode?: number | null;
  lastMessage?: string;
}

interface MutableJobState {
  definition: OctocodeCronJobDefinition;
  enabled: boolean;
  status: OctocodeCronJobStatus;
  running: boolean;
  timer?: ReturnType<typeof setTimeout>;
  nextRunAt?: number;
  lastStartedAt?: number;
  lastFinishedAt?: number;
  lastExitCode?: number | null;
  lastMessage?: string;
}

export interface OctocodeCronRunResult {
  job: string;
  status: OctocodeCronJobStatus;
  exitCode?: number | null;
  message: string;
}

export interface OctocodeCronExecutor {
  (command: string, args: string[], opts: { signal?: AbortSignal; timeout?: number }): Promise<PiExecResult>;
}

export interface OctocodeCronScheduler {
  start(ctx?: PiContext): void;
  stop(): void;
  cancel(jobName?: string): string[];
  runNow(jobName: string | undefined, ctx?: PiContext): Promise<OctocodeCronRunResult[]>;
  list(): OctocodeCronJobSnapshot[];
}

export interface OctocodeCronSchedulerOptions {
  pi?: Pick<PiInstance, 'exec'>;
  executor?: OctocodeCronExecutor;
  env?: NodeJS.ProcessEnv;
  now?: () => number;
  /** Called after each job run (success or failure). Use for proactive TUI notifications or cache refreshes. */
  onJobComplete?: (result: OctocodeCronRunResult, ctx: PiContext | undefined) => void;
}

function parsePositiveInt(value: string | undefined, fallback: number): number {
  if (value === undefined || value.trim() === '') return fallback;
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function workspaceOf(ctx: PiContext | undefined): string {
  return ctx?.cwd ?? process.cwd();
}

function defaultJobs(env: NodeJS.ProcessEnv): OctocodeCronJobDefinition[] {
  return [
    {
      name: 'awareness-status',
      label: 'Awareness status',
      description: 'Report-first Awareness status summary (status prunes expired locks/work rows as a side effect).',
      intervalMs: parsePositiveInt(
        env['OCTOCODE_CRON_STATUS_INTERVAL_MS'],
        DEFAULT_AWARENESS_STATUS_INTERVAL_MS,
      ),
      enabledByDefault: env['OCTOCODE_CRON_STATUS'] !== '0',
      awarenessArgs: (ctx) => [
        'status',
        '--workspace',
        workspaceOf(ctx),
      ],
    },
  ];
}

function normalizeJobName(jobName: string | undefined): string {
  const trimmed = jobName?.trim();
  if (!trimmed || trimmed === 'default') return DEFAULT_CRON_JOB_NAME;
  return trimmed;
}

function selectJobs(states: Map<string, MutableJobState>, jobName: string | undefined): MutableJobState[] {
  const normalized = normalizeJobName(jobName);
  if (normalized === 'all' || normalized === '*') return [...states.values()];
  const state = states.get(normalized);
  return state ? [state] : [];
}

function truncateOutput(text: string, maxChars = 1200): string {
  const trimmed = text.trim();
  if (trimmed.length <= maxChars) return trimmed;
  return `${trimmed.slice(0, maxChars)}…`;
}

function formatDuration(ms: number): string {
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.round(minutes / 60)}h`;
}

function makeExecutor(pi: Pick<PiInstance, 'exec'> | undefined): OctocodeCronExecutor {
  if (pi?.exec) return (command, args, opts) => pi.exec!(command, args, opts);
  return (command, args, opts) =>
    new Promise<PiExecResult>((resolve) => {
      const child = execFile(
        command,
        args,
        {
          timeout: opts.timeout,
          signal: opts.signal,
          maxBuffer: 1024 * 1024,
        },
        (error, stdout, stderr) => {
          const errorCode = (error as NodeJS.ErrnoException | null)?.code;
          const code = typeof errorCode === 'number' ? errorCode : error ? 1 : 0;
          resolve({ stdout, stderr, code });
        },
      );
      opts.signal?.addEventListener('abort', () => child.kill(), { once: true });
    });
}

export function createOctocodeCronScheduler(
  options: OctocodeCronSchedulerOptions = {},
): OctocodeCronScheduler {
  const env = options.env ?? process.env;
  const now = options.now ?? Date.now;
  // Default execution is IN-PROCESS (no child process). An explicit executor or a
  // pi.exec seam (tests, foreign hosts) opts back into subprocess spawning and
  // preserves the `node cli.js …` spec assertions those callers make.
  const useSubprocess = Boolean(options.executor || options.pi?.exec);
  const executor = options.executor ?? makeExecutor(options.pi);
  const states = new Map<string, MutableJobState>();
  let active = false;
  let lastCtx: PiContext | undefined;

  for (const definition of defaultJobs(env)) {
    states.set(definition.name, {
      definition,
      enabled: definition.enabledByDefault,
      status: definition.enabledByDefault ? 'idle' : 'cancelled',
      running: false,
    });
  }

  const clearTimer = (state: MutableJobState): void => {
    if (state.timer) clearTimeout(state.timer);
    state.timer = undefined;
    state.nextRunAt = undefined;
  };

  const schedule = (state: MutableJobState): void => {
    clearTimer(state);
    if (!active || !state.enabled || state.running) return;
    const delay = state.definition.intervalMs;
    state.nextRunAt = now() + delay;
    state.status = 'scheduled';
    state.timer = setTimeout(() => {
      void runOne(state.definition.name, lastCtx, true);
    }, delay);
    (state.timer as { unref?: () => void }).unref?.();
  };

  const runOne = async (
    jobName: string,
    ctx: PiContext | undefined,
    rescheduleAfterRun: boolean,
  ): Promise<OctocodeCronRunResult> => {
    const state = states.get(jobName);
    if (!state) {
      return { job: jobName, status: 'failed', message: `Unknown cron job: ${jobName}` };
    }
    clearTimer(state);
    if (state.running) {
      return { job: jobName, status: 'skipped', message: `${state.definition.label} is already running.` };
    }

    state.running = true;
    state.status = 'running';
    state.lastStartedAt = now();
    state.lastMessage = undefined;
    let runResult: OctocodeCronRunResult | undefined;
    try {
      const args = state.definition.awarenessArgs(ctx);
      let result: PiExecResult;
      if (useSubprocess) {
        const spec = buildAwarenessCommand(args);
        result = await executor(spec.cmd, spec.args, { timeout: DEFAULT_JOB_TIMEOUT_MS });
      } else {
        const r = runAwarenessInProcess(args);
        result = { stdout: r.stdout, stderr: r.stderr, code: r.code };
      }
      const output = truncateOutput([result.stdout, result.stderr].filter(Boolean).join('\n'));
      state.lastExitCode = result.code;
      state.status = result.code === 0 ? 'succeeded' : 'failed';
      state.lastMessage = output || (result.code === 0 ? 'completed' : `exited with ${result.code}`);
      runResult = { job: jobName, status: state.status, exitCode: result.code, message: state.lastMessage };
      return runResult;
    } catch (error) {
      state.lastExitCode = 1;
      state.status = 'failed';
      state.lastMessage = error instanceof Error ? error.message : String(error);
      runResult = { job: jobName, status: 'failed', exitCode: 1, message: state.lastMessage };
      return runResult;
    } finally {
      state.running = false;
      state.lastFinishedAt = now();
      if (rescheduleAfterRun) schedule(state);
      if (runResult) options.onJobComplete?.(runResult, ctx);
    }
  };

  return {
    start(ctx?: PiContext): void {
      lastCtx = ctx;
      active = env['OCTOCODE_CRON'] !== '0';
      for (const state of states.values()) {
        if (active && state.enabled) schedule(state);
        else if (!active) {
          clearTimer(state);
          state.status = 'cancelled';
        }
      }
    },

    stop(): void {
      active = false;
      for (const state of states.values()) clearTimer(state);
    },

    cancel(jobName?: string): string[] {
      const targets = selectJobs(states, jobName);
      for (const state of targets) {
        state.enabled = false;
        state.status = 'cancelled';
        clearTimer(state);
      }
      return targets.map((state) => state.definition.name);
    },

    async runNow(jobName?: string, ctx?: PiContext): Promise<OctocodeCronRunResult[]> {
      lastCtx = ctx ?? lastCtx;
      const normalized = normalizeJobName(jobName);
      const targets = selectJobs(states, jobName);
      if (targets.length === 0) {
        return [{ job: normalized, status: 'failed', message: `Unknown cron job: ${normalized}` }];
      }
      const results: OctocodeCronRunResult[] = [];
      for (const state of targets) results.push(await runOne(state.definition.name, lastCtx, active && state.enabled));
      return results;
    },

    list(): OctocodeCronJobSnapshot[] {
      return [...states.values()].map((state) => ({
        name: state.definition.name,
        label: state.definition.label,
        description: state.definition.description,
        intervalMs: state.definition.intervalMs,
        enabled: state.enabled && active,
        status: state.status,
        running: state.running,
        nextRunAt: state.nextRunAt === undefined ? undefined : new Date(state.nextRunAt).toISOString(),
        lastStartedAt: state.lastStartedAt === undefined ? undefined : new Date(state.lastStartedAt).toISOString(),
        lastFinishedAt: state.lastFinishedAt === undefined ? undefined : new Date(state.lastFinishedAt).toISOString(),
        lastExitCode: state.lastExitCode,
        lastMessage: state.lastMessage,
      }));
    },
  };
}

export function formatOctocodeCronSummary(snapshots: OctocodeCronJobSnapshot[]): string {
  if (snapshots.length === 0) return 'session jobs: none';
  const enabled = snapshots.filter((job) => job.enabled).length;
  const running = snapshots.filter((job) => job.running).length;
  const failed = snapshots.filter((job) => job.status === 'failed').length;
  const scheduled = snapshots.filter((job) => job.status === 'scheduled').length;
  return `session jobs: ${enabled}/${snapshots.length} enabled · ${scheduled} scheduled · ${running} running · ${failed} failed`;
}

export function formatOctocodeCronStatus(snapshots: OctocodeCronJobSnapshot[]): string {
  const lines = ['Octocode session jobs', '', formatOctocodeCronSummary(snapshots), ''];
  if (snapshots.length === 0) return 'Octocode session jobs\n\n(no jobs registered)';
  for (const job of snapshots) {
    lines.push(`${job.enabled ? '✓' : '–'} ${job.name} — ${job.status}`);
    lines.push(`  ${job.description}`);
    lines.push(`  interval: ${formatDuration(job.intervalMs)}`);
    if (job.nextRunAt) lines.push(`  next: ${job.nextRunAt}`);
    if (job.lastFinishedAt) lines.push(`  last: ${job.lastFinishedAt} (${job.lastExitCode ?? 'n/a'})`);
    if (job.lastMessage) lines.push(`  message: ${job.lastMessage}`);
  }
  lines.push('', 'Commands: /octocode-cron list · check [default|all|job] · cancel [default|all|job] · help');
  return lines.join('\n');
}

export const OCTOCODE_CRON_COMMAND_COMPLETIONS = [
  'list',
  'check',
  'check all',
  'cancel',
  'cancel all',
  'help',
] as const;

export const OCTOCODE_CRON_COMMAND_USAGE = 'list|check [default|all|job]|cancel [default|all|job]|help';

function formatRunResults(results: OctocodeCronRunResult[]): string {
  return [
    'Octocode session job check',
    '',
    ...results.map((result) => {
      const exit = result.exitCode === undefined ? '' : ` exit=${result.exitCode}`;
      return `${result.job}: ${result.status}${exit}\n${result.message}`;
    }),
  ].join('\n');
}

export async function handleOctocodeCronCommand(
  args: string,
  ctx: PiContext | undefined,
  scheduler: OctocodeCronScheduler,
  notify: (ctx: PiContext | undefined, message: string, level?: string) => void,
): Promise<void> {
  const [command = 'list', target] = args.trim().split(/\s+/).filter(Boolean);
  switch (command) {
    case 'list':
      notify(ctx, formatOctocodeCronStatus(scheduler.list()), 'info');
      return;
    case 'check': {
      const results = await scheduler.runNow(target, ctx);
      notify(ctx, formatRunResults(results), results.some((result) => result.status === 'failed') ? 'warning' : 'info');
      return;
    }
    case 'cancel': {
      const cancelled = scheduler.cancel(target);
      notify(ctx, `Cancelled Octocode session job(s): ${cancelled.join(', ') || '(none)'}`, 'info');
      return;
    }
    case 'help':
      notify(ctx, `Usage: /octocode-cron ${OCTOCODE_CRON_COMMAND_USAGE}`, 'info');
      return;
    default:
      notify(ctx, `Unknown /octocode-cron command: ${command}\nUsage: /octocode-cron ${OCTOCODE_CRON_COMMAND_USAGE}`, 'warning');
  }
}
