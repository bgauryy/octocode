import fs from 'node:fs';
import path from 'node:path';
import { createBashToolDefinition, type ExtensionAPI, type ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';
import { wordCompletions, type Subcommands } from '../shared/commands.js';
import { envInt } from '../shared/env.js';
import { PRIVATE_FILE_MODE, moveFile } from '../shared/home.js';
import { formatDuration, shortPath } from '../shared/format.js';
import { statusColor, timedTool, toolHeader } from '../shared/render.js';
import { renderBashResult, renderJobMessage, type BashDetails } from './render.js';
import { contentText, errorMessage, isRecord } from '../shared/util.js';
import { BashJobs, JOB_TYPE, MAX_TIMER_SECONDS, jobReport, shellEnv, shellSettings, type BashJob, type JobExit, type ShellSettings } from './bash-jobs.js';

/**
 * Pi's `bash`, with two changes: a waited-for command always has a deadline (Pi's has none, so a server, watcher or
 * prompt hangs the turn until the user presses Esc), and `background: true` starts a job whose exit arrives later as
 * a message. The foreground path is Pi's own tool (streaming, truncation, the live `Elapsed` timer, the user's
 * `shellPath` and `shellCommandPrefix` settings), only with the timeout filled in. Background jobs use the same shell,
 * prefix and environment. Job logs and the full output of a truncated command live in the session's `bash/` folder
 * (`sessionOutputDir`), never in the shared temp directory.
 */

const BASH_TIMEOUT_ENV = 'OCTOCODE_BASH_TIMEOUT';
/** Longest wait for a foreground command, in seconds. */
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

/** The foreground deadline in seconds: `OCTOCODE_BASH_TIMEOUT` (a positive integer), else 15 minutes. */
export function bashTimeoutLimit(env: NodeJS.ProcessEnv = process.env): number {
  const value = envInt(env, BASH_TIMEOUT_ENV, DEFAULT_BASH_TIMEOUT);
  // Node timers fire at once past ~24.8 days, which would stop every command immediately.
  return value >= 1 ? Math.min(value, MAX_TIMER_SECONDS) : DEFAULT_BASH_TIMEOUT;
}

/** The timeout a call runs with: what it asked for, never above `limit`; the limit when it asked for none. */
export function effectiveTimeout(requested: unknown, limit: number): number {
  return typeof requested === 'number' && Number.isFinite(requested) && requested > 0 ? Math.min(requested, limit) : limit;
}

export function registerBashTool(
  pi: ExtensionAPI,
  commands?: Subcommands,
  jobs = new BashJobs(),
  settingsFor: (ctx: ExtensionContext) => ShellSettings = (ctx) => shellSettings(ctx.cwd, ctx.isProjectTrusted()),
): BashJobs {
  // Pi's bash for the settings in force; rebuilt only when they change.
  let builtin = createBashToolDefinition(process.cwd());
  let builtFor = '{}';
  const builtinFor = (settings: ShellSettings) => {
    const key = JSON.stringify(settings);
    if (key !== builtFor) {
      builtin = createBashToolDefinition(process.cwd(), settings);
      builtFor = key;
    }
    return builtin;
  };
  const limit = bashTimeoutLimit();
  let current: ExtensionContext | undefined;
  const showJobs = () => {
    try {
      if (current) current.ui.setStatus('octocode-bash', jobs.jobs.size > 0 ? statusColor(current, 'accent', `${jobs.jobs.size} bash job${jobs.jobs.size === 1 ? '' : 's'}`) : undefined);
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
    if (job.stopped === 'session end') return;
    report(job, exit, true);
  };

  pi.on('session_start', async (_event, ctx) => {
    current = ctx;
  });
  pi.on('session_shutdown', async () => {
    // Reported before stopping, without waking anyone: a resumed session then knows they did not finish.
    const now = Date.now();
    for (const job of jobs.jobs.values()) {
      job.stopped ??= 'session end';
      report(job, { code: null, signal: null, seconds: Math.round((now - job.startedAt) / 1000) }, false);
    }
    jobs.stopAll('session end');
    current = undefined;
  });
  pi.registerMessageRenderer(JOB_TYPE, (message, { expanded, outputPad }, theme) => renderJobMessage(contentText(message.content), expanded, theme, outputPad ?? 1));

  commands?.add('jobs', {
    description: 'jobs [kill <id>] — list or stop background bash jobs',
    handler: async (args, ctx) => {
      const [sub = '', id = ''] = args.split(/\s+/);
      if (!sub) return ctx.ui.notify(jobs.describe(), 'info');
      if (sub !== 'kill') return ctx.ui.notify('Usage: /octocode jobs [kill <id>]', 'warning');
      if (!jobs.stop(id, 'user')) return ctx.ui.notify(`No background bash job "${id}".\n${jobs.describe()}`, 'warning');
      ctx.ui.notify(`Stopping ${id}…`, 'info');
    },
    complete: (prefix) => {
      if (!prefix.startsWith('kill ')) return wordCompletions([['kill', 'kill <id> — stop a background job']], prefix)?.map((item) => ({ ...item, value: 'kill ' })) ?? null;
      return wordCompletions([...jobs.jobs.keys()], prefix.slice(5).trimStart())?.map((item) => ({ ...item, value: `kill ${item.value}` })) ?? null;
    },
  });

  const limitText = formatDuration(limit * 1000);
  pi.registerTool(timedTool({
    name: 'bash',
    label: 'bash',
    description:
      'Run shell commands in the working directory: builds, tests, git, formatters and installed CLIs. Foreground calls wait for completion and return combined output; large output keeps a tail and names the full log. ' +
      'Background calls return a job id, pid and log path; completion arrives automatically as a message. A timeout does not undo side effects: inspect the result before rerunning a command.',
    // Pi's own snippet ("ls, grep, find") would contradict the Octocode research routing.
    promptSnippet: 'Run builds, tests, git and other shell commands (background jobs for long ones)',
    ...(builtin.constrainedSampling !== undefined ? { constrainedSampling: builtin.constrainedSampling } : {}),
    promptGuidelines: [
      ...(builtin.promptGuidelines ?? []),
      `Foreground bash stops after at most ${limitText}. Use \`background: true\` for work that needs longer; read its log when it affects your next step, and stop unneeded processes with \`kill -- -<pid>\`.`,
    ],
    parameters: Type.Object({
      command: Type.String({ description: 'Shell command to execute' }),
      timeout: Type.Optional(Type.Number({ description: `Timeout in seconds; foreground at most ${limit} (the default). A background job has none unless given` })),
      background: Type.Optional(Type.Boolean({ description: 'Start the command and return at once; its exit arrives later as a message. For servers, watchers and long builds' })),
    }),
    async execute(toolCallId, params, signal, onUpdate, ctx) {
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
        return { content: [{ type: 'text' as const, text }], details: { job: job.id, pid: job.pid, log: job.log } as BashDetails };
      }
      const timeout = effectiveTimeout(params.timeout, limit);
      // Pi reports the spill file in its updates' details; a timeout or abort throws, and only the updates name it then.
      let spilled: string | undefined;
      try {
        const result = await builtinFor(settings).execute(
          toolCallId,
          { command: params.command, timeout },
          signal,
          (update) => {
            spilled = update.details?.fullOutputPath ?? spilled;
            onUpdate?.(update);
          },
          ctx,
        );
        const details: BashDetails = { ...result.details };
        if (!details.fullOutputPath) return { ...result, details };
        const moved = adoptFullOutput(contentText(result.content), details.fullOutputPath, jobs.logDir());
        if (!moved.path) return { ...result, details };
        const output = result.structuredContent;
        const structured = isRecord(output) && output['full_output_path'] ? { structuredContent: { ...output, full_output_path: moved.path } } : {};
        return { ...result, ...structured, content: [{ type: 'text' as const, text: moved.text }], details: { ...details, fullOutputPath: moved.path } };
      } catch (error) {
        const message = adoptFullOutput(errorMessage(error), spilled, jobs.logDir()).text;
        if (!/Command timed out after/.test(message)) throw new Error(message);
        throw new Error(`${message}.\nIf it needs longer, rerun it with background: true and check its log; if it waits for input, pass the input or a non-interactive flag.`);
      }
    },
    renderCall(args, theme, context) {
      // The timeout shows only when the model set one; the default deadline is in the tool description.
      const asked = typeof args.timeout === 'number' && args.timeout > 0 ? args.timeout : undefined;
      const timeout = asked ? `timeout ${formatDuration((args.background ? asked : effectiveTimeout(asked, limit)) * 1000)}` : '';
      const meta = [args.background ? 'background' : '', timeout].filter(Boolean).join(' · ');
      return toolHeader(theme, context, 'Bash', typeof args.command === 'string' ? args.command : '', meta ? { meta } : {});
    },
    renderResult: renderBashResult,
  }));
  return jobs;
}
