import type { ExtensionAPI, ExtensionContext, ToolCallEvent, ToolCallEventResult } from '@earendil-works/pi-coding-agent';
import { Box, Text } from '@earendil-works/pi-tui';
import { formatDuration } from '../shared/format.js';
import { resultBlock } from '../shared/render.js';
import type { Subcommands } from '../shared/commands.js';
import { HOOKS_ENV, envFlag } from '../shared/env.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { configFileLines, findRepoRoot, tildePath } from '../shared/home.js';
import { projectTrust } from '../shared/trust.js';
import { clipText, contentText, errorMessage } from '../shared/util.js';
import { commandsFor, emptyHooks, HOOK_EVENTS, hookCalls, hookFiles, loadHooks, type HookCommand, type HookConfig, type HookEvent } from './config.js';
import { capContext, interpret, runHook, type HookVerdict } from './runner.js';

/**
 * Optional adapter for Claude Code / Codex command hooks, adapted from the native host's hook dispatcher:
 * PreToolUse → `tool_call` (can block), PostToolUse → `tool_result` (can add feedback), SessionStart (also after a
 * compaction, source `compact`) → context for the next prompt, PreCompact → `session_before_compact` (side effects
 * only), Stop → `agent_before_settle` (a block makes the agent continue with the reason), Notification → when Pi asks
 * the user something or an answer waits (side effects only). Off unless `OCTOCODE_HOOKS=1`, because hooks written for
 * another agent run arbitrary commands.
 */
const CONTEXT_TYPE = 'octocode-hook-context';
/** Longest the first prompt waits for SessionStart hooks still running. */
const START_HOOKS_WAIT_MS = 15_000;
const STATUS_KEY = 'octocode-hooks';
const STOP_TYPE = 'octocode-hook-stop';
const STOP_PREFIX = 'Stop hook feedback:\n';
/** Claude Code's cap on consecutive Stop-hook continuations; a tool call resets the count. */
const STOP_CONTINUE_CAP = 8;
/** A hook run this slow is reported once: PreToolUse hooks hold back every call of the batch. */
const SLOW_HOOK_MS = 5_000;

/** Time each hook command took, for `/octocode hooks`. */
interface HookTiming {
  runs: number;
  totalMs: number;
  maxMs: number;
}

/** What a Notification hook receives besides the common fields (Claude Code's shape). */
export interface HookNotice {
  message: string;
  title?: string;
  /** Claude Code's `notification_type`: `permission_prompt`, `elicitation_dialog` or `idle_prompt`. */
  type: string;
}

const hooksEnabled = (env: NodeJS.ProcessEnv = process.env): boolean => envFlag(env, HOOKS_ENV);

/** Pi's session start reasons in Claude Code's SessionStart matcher vocabulary. */
const START_SOURCE: Record<string, string> = { startup: 'startup', reload: 'startup', new: 'clear', resume: 'resume', fork: 'resume' };

const hookCount = (config: HookConfig): number => HOOK_EVENTS.reduce((sum, event) => sum + config[event].reduce((n, group) => n + group.hooks.length, 0), 0);

/** What `/hooks` and `/octocode` show about hooks. */
interface HooksView {
  /** One line for the `/octocode` status. */
  summary(): string;
  /** On/off, the loaded commands per event, the files read (✓ = exists) and any config errors. */
  report(cwd: string): string;
  /** The PreToolUse hooks as a tool-call gate (a no-op while hooks are off). */
  preToolUse(event: ToolCallEvent, ctx: ExtensionContext): Promise<ToolCallEventResult | undefined>;
  /** Runs the Notification hooks matching `notice.type` in the background (a no-op while hooks are off). */
  notification(notice: HookNotice, ctx: ExtensionContext): void;
}

function timingText(timing: HookTiming | undefined): string {
  if (!timing) return '';
  return ` · ${timing.runs} run${timing.runs === 1 ? '' : 's'}, avg ${formatDuration(timing.totalMs / timing.runs)}, max ${formatDuration(timing.maxMs)}`;
}

function hooksReport(enabled: boolean, config: HookConfig, errors: string[], cwd: string, home?: string, timings = new Map<string, HookTiming>()): string {
  const loaded = HOOK_EVENTS.flatMap((event) =>
    config[event].flatMap((group) =>
      group.hooks.map((hook) => {
        const command = clipText(hook.command, 80);
        return `  ${event} [${group.matcher || '*'}] ${command} (${tildePath(hook.source, home)})${timingText(timings.get(`${event}\0${hook.command}`))}`;
      }),
    ),
  );
  return [
    enabled ? `Hooks: on (${HOOKS_ENV}=1)` : `Hooks: off. Start Pi with ${HOOKS_ENV}=1 to run them.`,
    ...(enabled ? [`Loaded (${loaded.length}):`, ...(loaded.length > 0 ? loaded : ['  none'])] : []),
    '',
    `Hook files (Claude Code / Codex format, {"hooks": {...}}; events ${HOOK_EVENTS.join(', ')}):`,
    ...configFileLines(hookFiles(cwd, true, home), hookFiles(cwd, false, home), home),
    ...(errors.length > 0 ? ['', 'Invalid files (ignored):', ...errors.map((error) => `  ${error}`)] : []),
  ].join('\n');
}

export function registerHooks(pi: ExtensionAPI, env: NodeJS.ProcessEnv = process.env, home?: string): HooksView {
  let config: HookConfig = emptyHooks();
  let errors: string[] = [];
  const enabled = hooksEnabled(env);
  const timings = new Map<string, HookTiming>();
  const view: HooksView = {
    summary: () => (enabled ? `on, ${hookCount(config)} commands` : `off (${HOOKS_ENV}=1 to enable)`),
    report: (cwd) => hooksReport(enabled, config, errors, cwd, home, timings),
    preToolUse: async () => undefined,
    notification: () => undefined,
  };
  // Registered even when hooks are off: a resumed session may hold Stop feedback from an earlier run.
  registerStopRenderer(pi);
  if (!enabled) return view;
  let pending: string[] = [];
  /** SessionStart hook commands still running: the next prompt waits for their context, session start does not. */
  let starting: Promise<void> | undefined;
  /** Ends the current session's SessionStart hooks (on shutdown or the next session start), so none outlive it. */
  let startAbort = new AbortController();
  /** The repository root for CLAUDE_PROJECT_DIR, found once per session and only when a hook actually runs. */
  let root: string | undefined;
  /** Stop-hook continuations in a row (reset by a prompt or a tool call). */
  let stopContinues = 0;
  /** A failed SessionStart dispatch is reported once per Pi process, not on every session start. */
  let startWarned = false;

  /** Commands already reported as slow, so each is reported once per Pi process. */
  const slowWarned = new Set<string>();
  const timeHook = async (ctx: ExtensionContext, event: HookEvent, hook: HookCommand, run: () => ReturnType<typeof runHook>): ReturnType<typeof runHook> => {
    const started = Date.now();
    const result = await run();
    const ms = Date.now() - started;
    const key = `${event}\0${hook.command}`;
    const timing = timings.get(key) ?? { runs: 0, totalMs: 0, maxMs: 0 };
    timings.set(key, { runs: timing.runs + 1, totalMs: timing.totalMs + ms, maxMs: Math.max(timing.maxMs, ms) });
    if (ms >= SLOW_HOOK_MS && !slowWarned.has(key) && ctx.hasUI && event !== 'SessionStart' && event !== 'Notification') {
      slowWarned.add(key);
      const why = event === 'PreToolUse' ? ': every tool call in a batch waits for it' : '';
      ctx.ui.notify(`Slow ${event} hook (${formatDuration(ms)})${why}. ${sanitizeTerminalText(clipText(hook.command, 80))}`, 'warning');
    }
    return result;
  };

  /**
   * Runs the hooks matching `names` in parallel, as Claude Code does; any block blocks, and verdicts combine in
   * config order. Most tool calls match no hook, so the input (which may join a large tool result) is built only when
   * one does.
   */
  const dispatch = async (ctx: ExtensionContext, event: HookEvent, names: string[], input: () => Record<string, unknown>, signal: AbortSignal | undefined = ctx.signal): Promise<HookVerdict> => {
    const hooks = commandsFor(config, event, names);
    if (hooks.length === 0) return {};
    root ??= findRepoRoot(ctx.cwd);
    const payload = { session_id: ctx.sessionManager.getSessionId(), transcript_path: ctx.sessionManager.getSessionFile() ?? '', cwd: ctx.cwd, hook_event_name: event, ...input() };
    const env = { CLAUDE_PROJECT_DIR: root };
    const verdicts = await Promise.all(hooks.map(async (hook) => interpret(event, await timeHook(ctx, event, hook, () => runHook(hook, payload, ctx.cwd, env, signal)))));
    const block = verdicts.map((verdict) => verdict.block).filter(Boolean).join('\n\n');
    const context = verdicts.map((verdict) => verdict.context).filter(Boolean).join('\n\n');
    return { ...(block ? { block: capContext(block) } : {}), ...(context ? { context: capContext(context) } : {}) };
  };

  /**
   * Starts the SessionStart hooks matching `source`. Hook commands can be slow (they spawn processes): they run
   * without holding up the event. A result that arrives after the session was replaced belongs to the old one and is
   * dropped. A session start ends the previous session's runs; a compaction's run joins any still going.
   */
  const startHooks = (ctx: ExtensionContext, source: string, fresh: boolean): void => {
    if (fresh) {
      startAbort.abort();
      startAbort = new AbortController();
    }
    const abort = startAbort;
    const previous = fresh ? undefined : starting;
    const hooks = dispatch(ctx, 'SessionStart', [source], () => ({ source }), abort.signal)
      .then((verdict) => {
        if (verdict.context && !abort.signal.aborted) pending.push(verdict.context);
      })
      .catch((error: unknown) => {
        if (startWarned || abort.signal.aborted) return;
        startWarned = true;
        if (ctx.hasUI) ctx.ui.notify(`SessionStart hooks could not run: ${sanitizeTerminalText(errorMessage(error))}`, 'warning');
      });
    const run: Promise<void> = (previous ? Promise.all([previous, hooks]) : hooks).then(() => undefined).finally(() => {
      if (starting === run) starting = undefined;
    });
    starting = run;
  };

  pi.on('session_start', async (event, ctx) => {
    const loaded = loadHooks(ctx.cwd, await projectTrust(ctx), home);
    config = loaded.config;
    errors = loaded.errors;
    pending = [];
    root = undefined;
    if (loaded.errors.length > 0 && ctx.hasUI) ctx.ui.notify(`Ignoring invalid hook config: ${loaded.errors.join('; ')}`, 'warning');
    startHooks(ctx, START_SOURCE[event.reason] ?? 'startup', true);
  });
  // Claude Code runs SessionStart again (source `compact`) after a compaction: its context comes with the next prompt.
  pi.on('session_compact', async (_event, ctx) => startHooks(ctx, 'compact', false));
  pi.on('session_shutdown', async () => startAbort.abort());

  // SessionStart context reaches the model once, with the next prompt, as data rather than instructions.
  pi.on('before_agent_start', async (_event, ctx) => {
    stopContinues = 0;
    // The first prompt waits a bounded time for their context; a slower hook's context comes with the next prompt.
    if (starting) {
      // Shown only while the prompt actually waits: `starting` clears itself once the hooks finish.
      if (ctx.hasUI) ctx.ui.setStatus(STATUS_KEY, 'Waiting for SessionStart hooks…');
      try {
        await Promise.race([starting, new Promise((resolve) => setTimeout(resolve, START_HOOKS_WAIT_MS).unref())]);
      } finally {
        if (ctx.hasUI) ctx.ui.setStatus(STATUS_KEY, undefined);
      }
    }
    if (pending.length === 0) return undefined;
    const content = capContext(`Context from SessionStart hooks:\n${pending.join('\n\n')}`);
    pending = [];
    return { message: { customType: CONTEXT_TYPE, content, display: false } };
  });

  // A `file` call is one dispatch per query (Edit / Write / Delete with Claude's input shape), all in parallel; any
  // block blocks the call.
  view.preToolUse = async (event, ctx) => {
    const calls = hookCalls(event.toolName, event.input, ctx.cwd);
    const verdicts = await Promise.all(calls.map((call) => dispatch(ctx, 'PreToolUse', call.names, () => ({ tool_name: call.tool_name, pi_tool_name: event.toolName, tool_input: call.tool_input }))));
    const block = verdicts.find((verdict) => verdict.block)?.block;
    return block ? { block: true, reason: `Blocked by a PreToolUse hook: ${block}` } : undefined;
  };

  view.notification = (notice, ctx) => {
    const input = { message: notice.message, ...(notice.title ? { title: notice.title } : {}), notification_type: notice.type };
    void dispatch(ctx, 'Notification', [notice.type], () => input, undefined).catch(() => undefined);
  };

  // Stop: when the agent would settle after a completed run, a blocking hook sends it on with the reason (Claude
  // Code's loop guards: `stop_hook_active` while continuing, at most 8 continuations in a row unless a tool runs).
  pi.on('tool_execution_start', async (event) => {
    if (!event.parentToolCallId) stopContinues = 0;
  });
  pi.on('agent_before_settle', async (event, ctx) => {
    if (event.outcome !== 'completed' || commandsFor(config, 'Stop', []).length === 0) return undefined;
    const last = [...event.context.llmMessages].reverse().find((message) => message.role === 'assistant');
    const verdict = await dispatch(ctx, 'Stop', [], () => ({ stop_hook_active: stopContinues > 0, last_assistant_message: last ? contentText(last.content) : '' }));
    if (!verdict.block || stopContinues >= STOP_CONTINUE_CAP) return undefined;
    stopContinues += 1;
    return { entries: [{ type: 'custom_message' as const, customType: STOP_TYPE, content: `${STOP_PREFIX}${verdict.block}`, display: true }], continue: true };
  });

  pi.on('tool_result', async (event, ctx) => {
    const feedback: string[] = [];
    for (const call of hookCalls(event.toolName, event.input, ctx.cwd)) {
      const verdict = await dispatch(ctx, 'PostToolUse', call.names, () => ({ tool_name: call.tool_name, pi_tool_name: event.toolName, tool_input: call.tool_input, tool_response: contentText(event.content) }));
      feedback.push(...[verdict.block, verdict.context].filter((text): text is string => Boolean(text)));
    }
    return feedback.length > 0 ? { content: [...event.content, { type: 'text' as const, text: `\n[PostToolUse hook] ${capContext(feedback.join('\n\n'))}` }] } : undefined;
  });

  pi.on('session_before_compact', async (event, ctx) => {

    const trigger = event.reason === 'manual' ? 'manual' : 'auto';
    await dispatch(ctx, 'PreCompact', [trigger], () => ({ trigger, custom_instructions: event.customInstructions ?? '' }));
    return undefined;
  });
  return view;
}

/** `/octocode hooks` (or `/hooks`): whether hooks run, what loaded, and which files to edit. */
/** `⚑ Stop hook`, then the hook's feedback: untrusted text, sanitized and previewed by resultBlock, never drawn as Markdown. */
function registerStopRenderer(pi: ExtensionAPI): void {
  pi.registerMessageRenderer(STOP_TYPE, (message, { expanded, outputPad }, theme) => {
    const text = contentText(message.content);
    const box = new Box(outputPad ?? 1, 0);
    box.addChild(new Text(`${theme.fg('warning', '⚑')} ${theme.fg('toolTitle', theme.bold('Stop hook'))} ${theme.fg('dim', '· continuing')}`, 0, 0));
    box.addChild(resultBlock(theme, { expanded }, { summary: '', body: text.startsWith(STOP_PREFIX) ? text.slice(STOP_PREFIX.length) : text, error: false }));
    return box;
  });
}

export function registerHooksCommand(commands: Subcommands, hooks: HooksView): void {
  commands.add('hooks', {
    description: 'hooks — hooks on/off, loaded commands and the files to configure',
    handler: async (_args, ctx) => ctx.ui.notify(sanitizeTerminalText(hooks.report(ctx.cwd)), 'info'),
  });
}
