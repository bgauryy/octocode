import fs from 'node:fs';
import path from 'node:path';
import { createBashToolDefinition, type ExtensionAPI, type ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';
import { say, wordCompletions, type Subcommands } from '../shared/commands.js';
import { SUBAGENT_ENV, envFlag } from '../shared/env.js';
import { PRIVATE_FILE_MODE } from '../shared/home.js';
import { formatDuration, shortPath } from '../shared/format.js';
import { plural, statusColor, toolHeader } from '../shared/render.js';
import { timedTool } from '../shared/tool-timing.js';
import { STATUS } from '../shared/status.js';
import { moveFile } from '../shared/sweep.js';
import { renderBashResult, renderJobMessage, RERUN_HINT, type BashDetails } from './render.js';
import { contentText, errorMessage, firstLine, isRecord, wholeSeconds } from '../shared/util.js';
import { BashJobs, JOB_TYPE, shellEnv, shellSettings, type BashJob, type JobExit, type ShellSettings } from './bash-jobs.js';
import { reapOrphanJobs, type OrphanJob } from './bash-orphans.js';
import { jobReport } from './bash-report.js';
import { foregroundOperations, strayNote } from './bash-run.js';

/**
 * Pi's `bash`, with two changes: a waited-for command has a deadline when the call names none (Pi's has none, so a
 * server, watcher or prompt hangs the turn until the user presses Esc), and `background: true` starts a job whose exit
 * arrives later as a message. The foreground path is Pi's own tool (streaming, truncation, the live `Elapsed` timer,
 * the user's `shellPath` and `shellCommandPrefix` settings), with the timeout filled in and its process handling from
 * bash-run.ts (a grace before SIGKILL, processes left running are named). Background jobs use the same shell, prefix
 * and environment. Job logs and the full output of a truncated command live in the session's `bash/` folder
 * (`sessionOutputDir`), never in the shared temp directory.
 */

/** `structuredContent` of a background start; a foreground command returns Pi's own bash output. */
const BACKGROUND_OUTPUT = Type.Object({
  job: Type.String({ description: 'Job id, for `/octocode jobs kill <id>`' }),
  pid: Type.Number({ description: 'Process group leader' }),
  log: Type.String({ description: 'Log file with the combined output' }),
});

/** Deadline of a foreground command whose call names none, in seconds. */
export const DEFAULT_BASH_TIMEOUT = 15 * 60;

/**
 * Pi writes a truncated command's full output (`from`, its `details.fullOutputPath`) to the OS temp directory; move it
 * under `dir` and point `text` at the new path. Unchanged when the move fails.
 */
function adoptFullOutput(text: string, from: string | undefined, dir: string): { text: string; path?: string } {
  if (!from || path.dirname(from) === dir || !fs.existsSync(from)) return { text };
  const to = path.join(dir, path.basename(from));
  if (fs.existsSync(to)) return { text };
  if (!moveFile(from, to)) return { text };
  fs.chmodSync(to, PRIVATE_FILE_MODE);
  return { text: text.split(from).join(shortPath(to)), path: to };
}

/** The longest a foreground command may hold a turn, in seconds; a longer one belongs in the background. */
export const MAX_FOREGROUND_TIMEOUT = 60 * 60;

/** What the result says when a foreground timeout was cut to `MAX_FOREGROUND_TIMEOUT`. */
const CLAMP_NOTE = `timeout clamped to ${MAX_FOREGROUND_TIMEOUT} s; use background: true for longer`;

/** The timeout a foreground call runs with: what it asked for (at most `MAX_FOREGROUND_TIMEOUT`), else `DEFAULT_BASH_TIMEOUT`. */
export function effectiveTimeout(requested: unknown): { seconds: number; clamped: boolean } {
  if (typeof requested !== 'number' || !Number.isFinite(requested) || requested <= 0) return { seconds: DEFAULT_BASH_TIMEOUT, clamped: false };
  return requested > MAX_FOREGROUND_TIMEOUT ? { seconds: MAX_FOREGROUND_TIMEOUT, clamped: true } : { seconds: requested, clamped: false };
}

/** The notice for jobs found running after their Pi died (`reapOrphanJobs`). */
function orphanNotice(orphans: readonly OrphanJob[]): string {
  const line = (job: OrphanJob) => `  ${job.id} · pid ${job.pid} · ${firstLine(job.command).slice(0, 80)}\n    log: ${shortPath(job.log)}`;
  const stopped = orphans.filter((job) => job.stopped);
  const unsure = orphans.filter((job) => !job.stopped);
  return [
    ...(stopped.length ? [`Stopped ${plural(stopped.length, 'background bash job')} left running by a Pi that exited without ending ${stopped.length === 1 ? 'it' : 'them'}:`, ...stopped.map(line)] : []),
    ...(unsure.length ? [`Left alone, since they could not be confirmed as these jobs (the pid may belong to another process now): ${plural(unsure.length, 'background bash job')} of a Pi that exited; stop one yourself with kill -- -<pid> if it is still the job:`, ...unsure.map(line)] : []),
  ].join('\n');
}

export function registerBashTool(
  pi: ExtensionAPI,
  commands?: Subcommands,
  jobs = new BashJobs(),
  settingsFor: (ctx: ExtensionContext) => ShellSettings = (ctx) => shellSettings(ctx.cwd, ctx.isProjectTrusted()),
): BashJobs {
  // Pi's bash, for its output schema; each call builds its own with that call's settings and process handling.
  const builtin = createBashToolDefinition(process.cwd());
  let current: ExtensionContext | undefined;
  // Set from session_shutdown: a job that ends meanwhile reports without waking anyone.
  let closing = false;
  const reportedAtShutdown = new WeakSet<BashJob>();
  const showJobs = () => {
    try {
      if (current) current.ui.setStatus(STATUS.bash, jobs.jobs.size > 0 ? statusColor(current, 'accent', `${jobs.jobs.size} bash job${jobs.jobs.size === 1 ? '' : 's'}`) : undefined);
    } catch {
      // The session was replaced.
    }
  };
  const report = (job: BashJob, exit: JobExit, wake: boolean) => {
    try {
      // followUp: read once the current turn ends; triggerTurn wakes an idle session. The log names the job uniquely
      // (ids restart with each extension instance), so a resumed session can tell which jobs reported.
      pi.sendMessage({ customType: JOB_TYPE, content: jobReport(job, exit), display: true, details: { id: job.id, log: job.log } }, { triggerTurn: wake, deliverAs: 'followUp' });
    } catch {
      // The session ended while the job ran.
    }
  };
  const onExit = (job: BashJob, exit: JobExit) => {
    showJobs();
    // Reported at session_shutdown already, while the session could still record it.
    if (reportedAtShutdown.has(job)) return;
    // A stop the user asked for is recorded for the model without starting a turn.
    report(job, exit, !closing && job.stopped !== 'user');
  };

  pi.on('session_start', async (event, ctx) => {
    current = ctx;
    closing = false;
    // Pi has no hook before a reload: the jobs it stopped are named here, from the note the shutdown left.
    const reloaded = event?.reason === 'reload' ? jobs.takeReloadNote() : [];
    if (reloaded.length > 0) say(ctx, [`/reload stopped ${plural(reloaded.length, 'background bash job')}:`, ...reloaded.map((job) => `  ${job.id} · ${firstLine(job.command).slice(0, 80)}\n    log: ${shortPath(job.log)}`)].join('\n'), 'warning');
    // Subagents leave this to the top-level Pi.
    if (envFlag(process.env, SUBAGENT_ENV)) return;
    const orphans = await reapOrphanJobs(jobs.recordDirs());
    if (orphans.length > 0) say(ctx, orphanNotice(orphans), 'warning');
  });
  pi.on('session_shutdown', async (event) => {
    closing = true;
    const reason = event?.reason === 'reload' ? 'reload' : 'session end';
    const now = Date.now();
    const running = jobs.running();
    // Reported before stopping, without waking anyone: a resumed session then knows they did not finish.
    for (const job of running) {
      job.stopped ??= reason;
      reportedAtShutdown.add(job);
      report(job, { code: null, signal: null, seconds: wholeSeconds(now - job.startedAt) }, false);
    }
    if (reason === 'reload') jobs.noteReload(running);
    // A job that exited just before reports its own status once its last output drained (within the drain time).
    const ending = [...jobs.jobs.values()].filter((job) => job.exited).map((job) => job.done);
    await Promise.all([jobs.stopAll(reason), ...ending]);
    current = undefined;
  });
  pi.registerMessageRenderer(JOB_TYPE, (message, { expanded, outputPad }, theme) => renderJobMessage(contentText(message.content), expanded, theme, outputPad ?? 1));

  commands?.add('jobs', {
    description: 'jobs [kill <id>] — list or stop background bash jobs',
    handler: async (args, ctx) => {
      const [sub = '', id = ''] = args.split(/\s+/);
      if (!sub) return say(ctx, jobs.describe(), 'info');
      if (sub !== 'kill') return say(ctx, 'Usage: /octocode jobs [kill <id>]', 'warning');
      if (!jobs.stop(id, 'user')) return say(ctx, `No background bash job "${id}".\n${jobs.describe()}`, 'warning');
      say(ctx, `Stopping ${id}…`, 'info');
    },
    complete: (prefix) => {
      if (!prefix.startsWith('kill ')) return wordCompletions([['kill', 'kill <id> — stop a background job']], prefix)?.map((item) => ({ ...item, value: 'kill ' })) ?? null;
      return wordCompletions([...jobs.jobs.keys()], prefix.slice(5).trimStart())?.map((item) => ({ ...item, value: `kill ${item.value}` })) ?? null;
    },
  });

  pi.registerTool(timedTool({
    name: 'bash',
    label: 'bash',
    description:
      'Run shell commands in the working directory: builds, tests, git, formatters and installed CLIs. Foreground calls wait for completion and return combined output; large output keeps a tail and names the full log. ' +
      'Background calls return a job id, pid and log path; completion arrives automatically as a message. Never sleep-poll for jobs or files. A timeout does not undo side effects: inspect the result before rerunning a command.',
    // Pi's own snippet ("ls, grep, find") would contradict the Octocode research routing.
    promptSnippet: 'Run shell commands: builds, tests, git',
    ...(builtin.constrainedSampling !== undefined ? { constrainedSampling: builtin.constrainedSampling } : {}),
    // Codemode scripts receive `structuredContent` only from tools that declare its schema.
    ...(builtin.outputSchema !== undefined ? { outputSchema: Type.Union([builtin.outputSchema, BACKGROUND_OUTPUT]) } : {}),
    // Pi's own guidelines (the PI_* environment hint) add nothing for the model, so none are set: the deadline lives in `timeout`, the stop command in a background start's result.
    promptGuidelines: [],
    parameters: Type.Object({
      command: Type.String({ description: 'Shell command to execute' }),
      timeout: Type.Optional(Type.Number({ description: `Timeout in seconds; foreground default ${DEFAULT_BASH_TIMEOUT}s, at most ${MAX_FOREGROUND_TIMEOUT}s. A background job has none unless given` })),
      background: Type.Optional(Type.Boolean({ description: 'Start the command and return at once; its exit arrives later as a message. For servers, watchers and long builds' })),
    }),
    async execute(toolCallId, params, signal, onUpdate, ctx) {
      signal?.throwIfAborted();
      current = ctx;
      const settings = settingsFor(ctx);
      if (params.background) {
        const job = await jobs.start(params.command, ctx.cwd, params.timeout, onExit, { env: shellEnv(ctx), shell: settings });
        const timeout = job.timeout;
        showJobs();
        const text = [
          `Started ${job.id} in the background (pid ${job.pid}${timeout ? `, stopped after ${formatDuration(timeout * 1000)}` : ''}). Keep working: its exit status and output tail arrive as a message.`,
          `Log: ${shortPath(job.log, ctx.cwd)}`,
          `Check: tail -n 40 ${shortPath(job.log, ctx.cwd)}`,
          `Stop: kill -- -${job.pid}`,
        ].join('\n');
        return { content: [{ type: 'text' as const, text }], structuredContent: { job: job.id, pid: job.pid, log: job.log }, details: { job: job.id, pid: job.pid, log: job.log } as BashDetails };
      }
      const { seconds: timeout, clamped } = effectiveTimeout(params.timeout);
      // Pi reports the spill file in its updates' details; a timeout or abort throws, and only the updates name it then.
      let spilled: string | undefined;
      let group: number | undefined;
      const run = createBashToolDefinition(process.cwd(), { ...settings, operations: foregroundOperations(settings, (pid) => (group = pid)) });
      try {
        const raw = await run.execute(
          toolCallId,
          { command: params.command, timeout },
          signal,
          (update) => {
            spilled = update.details?.fullOutputPath ?? spilled;
            onUpdate?.(update);
          },
          ctx,
        );
        const stray = group === undefined ? undefined : strayNote(group);
        const notes = [stray, clamped ? `(${CLAMP_NOTE})` : undefined].filter((note) => note !== undefined).join('\n\n');
        const result = notes ? { ...raw, content: [{ type: 'text' as const, text: `${contentText(raw.content)}\n\n${notes}` }] } : raw;
        const details: BashDetails = { ...result.details };
        if (!details.fullOutputPath) return { ...result, details };
        const moved = adoptFullOutput(contentText(result.content), details.fullOutputPath, jobs.logDir());
        if (!moved.path) return { ...result, details };
        const output = result.structuredContent;
        const structured = isRecord(output) && output['full_output_path'] ? { structuredContent: { ...output, full_output_path: moved.path } } : {};
        return { ...result, ...structured, content: [{ type: 'text' as const, text: moved.text }], details: { ...details, fullOutputPath: moved.path } };
      } catch (error) {
        const message = adoptFullOutput(errorMessage(error), spilled, jobs.logDir()).text;
        if (!/Command timed out after/.test(message)) throw new Error(clamped ? `${message}\n\n(${CLAMP_NOTE})` : message);
        const how = clamped ? `with background: true and check its log (${CLAMP_NOTE})` : 'with a longer timeout, or with background: true and check its log';
        throw new Error(`${message}.\n${RERUN_HINT} ${how}; if it waits for input, pass the input or a non-interactive flag.`);
      }
    },
    renderCall(args, theme, context) {
      // The timeout shows only when the model set one; the default deadline is in the tool description.
      const asked = typeof args.timeout === 'number' && args.timeout > 0 ? args.timeout : undefined;
      const timeout = asked ? `timeout ${formatDuration(asked * 1000)}` : '';
      const meta = [args.background ? 'background' : '', timeout].filter(Boolean).join(' · ');
      return toolHeader(theme, context, 'Bash', typeof args.command === 'string' ? args.command : '', meta ? { meta } : {});
    },
    renderResult: renderBashResult,
  }));
  return jobs;
}
