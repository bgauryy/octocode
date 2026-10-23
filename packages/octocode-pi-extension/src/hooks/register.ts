import type { ExtensionAPI, ExtensionContext, ToolCallEvent, ToolCallEventResult } from '@earendil-works/pi-coding-agent';
import { Box, Text } from '@earendil-works/pi-tui';
import { formatDuration } from '../shared/format.js';
import { STATUS } from '../shared/status.js';
import { resultBlock } from '../shared/render.js';
import { notify, say, type Subcommands } from '../shared/commands.js';
import { HOOKS_ENV, envFlag } from '../shared/env.js';
import { redactSecrets, sanitizeTerminalText, visibleControls } from '../shared/sanitize.js';
import { configFileLines, repoKey, tildePath } from '../shared/home.js';
import { clipText, contentText, errorMessage, firstLine, isRecord } from '../shared/util.js';
import { EXTERNAL_MESSAGE_TYPE, INTERNAL_MESSAGE_TYPE, type MessageEnvelope } from '../shared/messages.js';
import { commandsFor, emptyHooks, HOOK_EVENTS, hookCalls, hookFiles, type HookCommand, type HookConfig, type HookEvent } from './config.js';
import { capContext, failureOf, interpret, runHook } from './runner.js';
import { registerPostToolUseHook, registerPreCompactHook, registerSessionStartHooks } from './lifecycle.js';
import type { HookFailureSink, HookRunner, HookSession, HookTiming } from './types.js';

const STOP_TYPE = 'octocode-hook-stop';
const STOP_PREFIX = 'Stop hook feedback:\n';
/** Claude Code's cap on consecutive Stop-hook continuations; a tool call resets the count. */
const STOP_CONTINUE_CAP = 8;
/** A hook run this slow is reported once: PreToolUse hooks hold back every call of the batch. */
const SLOW_HOOK_MS = 5_000;

/**
 * Shows `label` in the footer status while `work` runs, with its elapsed time in the TUI. For hooks that run after Pi
 * removed its working indicator (Stop runs after `agent_end`): without it a slow hook looks like a finished, stuck run.
 */
async function withStatus<T>(ctx: ExtensionContext, label: string, work: () => Promise<T>): Promise<T> {
  if (!ctx.hasUI) return work();
  const started = Date.now();
  const show = () => {
    const ms = Date.now() - started;
    ctx.ui.setStatus(STATUS.hooks, `${label}${ms >= 1_000 ? ` · ${formatDuration(ms)}` : ''}`);
  };
  show();
  // RPC clients get the label once; only the TUI repaints the time.
  const timer = ctx.mode === 'tui' ? setInterval(show, 1_000) : undefined;
  timer?.unref?.();
  try {
    return await work();
  } finally {
    if (timer) clearInterval(timer);
    try {
      ctx.ui.setStatus(STATUS.hooks, undefined);
    } catch {
      // The session was replaced.
    }
  }
}

/** `Stop hook: <command>` (`+N` for the others): one line, its secrets redacted. */
function hooksLabel(event: HookEvent, hooks: HookCommand[]): string {
  const command = clipText(firstLine(redactSecrets(sanitizeTerminalText(hooks[0]?.command ?? ''))), 48);
  return `${event} hook: ${command}${hooks.length > 1 ? ` +${hooks.length - 1}` : ''}`;
}

/** What a Notification hook receives besides the common fields (Claude Code's shape). */
interface HookNotice {
  message: string;
  title?: string;
  /** Claude Code's `notification_type`: `permission_prompt`, `elicitation_dialog` or `idle_prompt`. */
  type: string;
}

const hooksEnabled = (env: NodeJS.ProcessEnv = process.env): boolean => envFlag(env, HOOKS_ENV);

const hookCount = (config: HookConfig): number => HOOK_EVENTS.reduce((sum, event) => sum + config[event].reduce((n, group) => n + group.hooks.length, 0), 0);

/** What `/hooks` and `/octocode` show about hooks. */
interface HooksView {
  /** One line for the `/octocode` status. */
  summary(): string;
  /** On/off, the loaded commands per event, the files read (✓ = exists) and any config errors. */
  report(cwd: string): string;
  /** The PreToolUse hooks as a tool-call gate (a no-op while hooks are off); `ask` carries a hook's ask to the permission gate. */
  preToolUse(event: ToolCallEvent, ctx: ExtensionContext): Promise<(ToolCallEventResult & { ask?: string }) | undefined>;
  /** Runs the Notification hooks matching `notice.type` in the background (a no-op while hooks are off). */
  notification(notice: HookNotice, ctx: ExtensionContext): void;
  messageSend(message: MessageEnvelope, ctx: ExtensionContext): Promise<{ block?: string; ask?: string } | undefined>;
  messageReceive(message: MessageEnvelope, ctx: ExtensionContext): Promise<void>;
}

function receivedMessages(value: unknown): MessageEnvelope[] {
  if (!isRecord(value) || value.role !== 'custom' || !isRecord(value.details)) return [];
  if (value.customType !== INTERNAL_MESSAGE_TYPE && value.customType !== EXTERNAL_MESSAGE_TYPE) return [];
  const envelope = value.details.message;
  if (isRecord(envelope) && (envelope.channel === 'internal' || envelope.channel === 'external') && typeof envelope.from === 'string' && typeof envelope.to === 'string' && typeof envelope.text === 'string' && typeof envelope.id === 'string') {
    return [{ channel: envelope.channel, direction: 'inbound', from: envelope.from, to: envelope.to, text: envelope.text, id: envelope.id, ...(typeof envelope.replyTo === 'string' ? { replyTo: envelope.replyTo } : {}) }];
  }
  if (value.customType !== INTERNAL_MESSAGE_TYPE || !Array.isArray(value.details.messages)) return [];
  return value.details.messages.flatMap((message): MessageEnvelope[] => {
    if (!isRecord(message) || typeof message.id !== 'number' || typeof message.from !== 'string' || typeof message.to !== 'string' || typeof message.text !== 'string') return [];
    return [{ channel: 'internal', direction: 'inbound', from: message.from, to: message.to, text: message.text, id: String(message.id), ...(typeof message.replyTo === 'number' ? { replyTo: String(message.replyTo) } : {}) }];
  });
}

const messageKey = (message: MessageEnvelope): string | undefined => message.id === undefined ? undefined : JSON.stringify([message.channel, message.direction, message.from, message.to, message.id]);
const messageNames = (message: MessageEnvelope): string[] => [message.channel, `${message.channel}.${message.direction}`];

function timingText(timing: HookTiming | undefined): string {
  if (!timing) return '';
  return ` · ${timing.runs} run${timing.runs === 1 ? '' : 's'}, avg ${formatDuration(timing.totalMs / timing.runs)}, max ${formatDuration(timing.maxMs)}`;
}

function hooksReport(enabled: boolean, config: HookConfig, errors: string[], cwd: string, home?: string, timings = new Map<string, HookTiming>()): string {
  const loaded = HOOK_EVENTS.flatMap((event) =>
    config[event].flatMap((group) =>
      group.hooks.map((hook) => {
        const command = clipText(hook.command, 80);
        return `  ${event} [${group.matcher || '*'}] ${command}${hook.failClosed ? ' [failClosed]' : ''} (${tildePath(hook.source, home)})${timingText(timings.get(`${event}\0${hook.command}`))}`;
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

/** The shared hook runner: `dispatch` runs the hooks of one event, `reportFailure` says once per event that a dispatch itself failed. */
function createDispatcher(session: HookSession, timings: Map<string, HookTiming>, onFailure: HookFailureSink | undefined): HookRunner {
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
      notify(ctx.ui, `Slow ${event} hook (${formatDuration(ms)})${why}. ${clipText(hook.command, 80)}`, 'warning');
    }
    return result;
  };

  /** Hooks already warned about for a failure, so each is shown once per Pi process (every failure is audited). */
  const failureWarned = new Set<string>();
  /**
   * A `PreToolUse` hook gave no answer. Hooks fail open, so the call goes ahead unless the hook is `failClosed`; either
   * way the user sees a warning (once per hook, with a UI) and the audit records it. Returns the block reason when failing closed.
   */
  const hookFailed = (ctx: ExtensionContext, hook: HookCommand, failure: string, target: string, event: 'PreToolUse' | 'PreMessageSend'): string | undefined => {
    const closed = hook.failClosed === true;
    const command = clipText(firstLine(redactSecrets(visibleControls(hook.command))), 80);
    onFailure?.([`${event} hook ${failure}: ${command}`, `${event === 'PreToolUse' ? 'tool' : 'message'}: ${target}`], closed ? 'hook failed; failClosed denied the call' : 'hook failed; hooks fail open, the call was allowed', !closed);
    const key = `${event}\0${hook.command}`;
    if (ctx.hasUI && !failureWarned.has(key)) {
      failureWarned.add(key);
      notify(ctx.ui, `${event} hook ${failure}: ${command}. ${closed ? 'The call was denied (failClosed).' : 'The call was allowed (hooks fail open); set "failClosed": true on the hook to deny instead.'}`, 'warning');
    }
    return closed ? `${event} hook ${failure} and is set to failClosed: ${command}` : undefined;
  };

  /**
   * Runs the hooks matching `names` in parallel, as Claude Code does; any block blocks, and verdicts combine in
   * config order. Most tool calls match no hook, so the input (which may join a large tool result) is built only when
   * one does.
   */
  const dispatch: HookRunner['dispatch'] = async (ctx, event, names, input, signal = ctx.signal) => {
    const hooks = commandsFor(session.config, event, names);
    if (hooks.length === 0) return {};
    session.root ??= repoKey(ctx.cwd).root;
    const payload: Record<string, unknown> = { session_id: ctx.sessionManager.getSessionId(), transcript_path: ctx.sessionManager.getSessionFile() ?? '', cwd: ctx.cwd, hook_event_name: event, ...input() };
    const env = { CLAUDE_PROJECT_DIR: session.root };
    const verdicts = await Promise.all(
      hooks.map(async (hook) => {
        const run = await timeHook(ctx, event, hook, () => runHook(hook, payload, ctx.cwd, env, signal));
        const verdict = interpret(event, run);
        const gate = event === 'PreToolUse' || event === 'PreMessageSend' ? event : undefined;
        const failure = gate ? failureOf(hook, run, signal?.aborted === true) : undefined;
        const message = isRecord(payload['message']) ? payload['message'] : {};
        const target = gate === 'PreMessageSend' ? `${message.from} -> ${message.to}` : String(payload['tool_name'] ?? '');
        const block = failure && gate ? hookFailed(ctx, hook, failure, target, gate) : undefined;
        return block ? { ...verdict, block } : verdict;
      }),
    );
    const block = verdicts.map((verdict) => verdict.block).filter(Boolean).join('\n\n');
    const context = verdicts.map((verdict) => verdict.context).filter(Boolean).join('\n\n');
    const ask = verdicts.map((verdict) => verdict.ask).filter(Boolean).join('\n\n');
    return { ...(block ? { block: capContext(block) } : {}), ...(context ? { context: capContext(context) } : {}), ...(ask ? { ask: capContext(ask) } : {}) };
  };

  /** A dispatch that itself failed (not a hook's verdict): reported once per event per Pi process, with a UI. */
  const dispatchWarned = new Set<HookEvent>();
  const reportFailure: HookRunner['reportFailure'] = (ctx, event, error) => {
    if (dispatchWarned.has(event)) return;
    dispatchWarned.add(event);
    try {
      if (ctx.hasUI) notify(ctx.ui, `${event} hooks could not run: ${errorMessage(error)}`, 'warning');
    } catch {
      // The session was replaced.
    }
  };

  return { dispatch, reportFailure };
}

/**
 * A `file` call is one dispatch per query (Edit / Write / Delete with Claude's input shape), all in parallel; any
 * block blocks the call.
 */
const preToolUseGate = (dispatch: HookRunner['dispatch']): HooksView['preToolUse'] => async (event, ctx) => {
  const calls = hookCalls(event.toolName, event.input, ctx.cwd);
  const verdicts = await Promise.all(calls.map((call) => dispatch(ctx, 'PreToolUse', call.names, () => ({ tool_name: call.tool_name, pi_tool_name: event.toolName, tool_input: call.tool_input }))));
  const block = verdicts.find((verdict) => verdict.block)?.block;
  if (block) return { block: true, reason: `Blocked by a PreToolUse hook: ${block}` };
  const ask = verdicts.map((verdict) => verdict.ask).filter(Boolean).join('\n\n');
  return ask ? { ask: capContext(ask) } : undefined;
};

/**
 * PreMessageSend gates a send; PostMessageReceive observes delivery. Tracks which session the messages belong to and
 * which were already seen (a resumed session replays its history), so each received message dispatches once.
 */
function registerMessageHooks(pi: ExtensionAPI, { dispatch, reportFailure }: HookRunner): Pick<HooksView, 'messageSend' | 'messageReceive'> {
  let messageAbort = new AbortController();
  let messageSessionId: string | undefined;
  const received = new Set<string>();
  let receivedObjects = new WeakSet<MessageEnvelope>();

  pi.on('session_start', async (_event, ctx) => {
    messageAbort.abort();
    messageAbort = new AbortController();
    messageSessionId = undefined;
    received.clear();
    receivedObjects = new WeakSet();
    try {
      messageSessionId = ctx.sessionManager.getSessionId();
      for (const entry of ctx.sessionManager.getBranch?.() ?? []) {
        const message = entry.type === 'message' ? entry.message : entry.type === 'custom_message' ? { role: 'custom', ...entry } : undefined;
        for (const envelope of receivedMessages(message)) {
          const key = messageKey(envelope);
          if (key) received.add(key);
        }
      }
    } catch { messageAbort.abort(); }
  });
  pi.on('session_shutdown', async () => messageAbort.abort());

  const messageSend: HooksView['messageSend'] = async (message, ctx) => {
    if (ctx.sessionManager.getSessionId() !== messageSessionId) throw new Error('The message belongs to a different session.');
    const signal = ctx.signal ? AbortSignal.any([messageAbort.signal, ctx.signal]) : messageAbort.signal;
    signal.throwIfAborted();
    const verdict = await dispatch(ctx, 'PreMessageSend', messageNames(message), () => ({ message }), signal);
    signal.throwIfAborted();
    return verdict.block ? { block: `Blocked by a PreMessageSend hook: ${verdict.block}` } : verdict.ask ? { ask: verdict.ask } : undefined;
  };

  const messageReceive: HooksView['messageReceive'] = async (message, ctx) => {
    const signal = messageAbort.signal;
    if (signal.aborted) return;
    try { if (ctx.sessionManager.getSessionId() !== messageSessionId) return; }
    catch { return; }
    const key = messageKey(message);
    if (key ? received.has(key) : receivedObjects.has(message)) return;
    if (key) received.add(key);
    else receivedObjects.add(message);
    // Observers never change the message or add a reply, even if a hook prints a gate verdict.
    await dispatch(ctx, 'PostMessageReceive', messageNames(message), () => ({ message }), signal).catch((error: unknown) => {
      if (!signal.aborted) reportFailure(ctx, 'PostMessageReceive', error);
    });
  };

  pi.on('context', async (event, ctx) => {
    const signal = messageAbort.signal;
    for (const message of event.messages) {
      for (const envelope of receivedMessages(message)) {
        if (signal.aborted) return undefined;
        await messageReceive(envelope, ctx);
      }
    }
    return undefined;
  });

  return { messageSend, messageReceive };
}

/** Background: its own signal, so aborting the run (ctx.signal) does not cancel it; the hook timeout still applies. */
const notificationHook = ({ dispatch, reportFailure }: HookRunner): HooksView['notification'] => (notice, ctx) => {
  const input = { message: notice.message, ...(notice.title ? { title: notice.title } : {}), notification_type: notice.type };
  void dispatch(ctx, 'Notification', [notice.type], () => input, new AbortController().signal).catch((error: unknown) => reportFailure(ctx, 'Notification', error));
};

/**
 * Stop: when the agent would settle after a completed run, a blocking hook sends it on with the reason (Claude
 * Code's loop guards: `stop_hook_active` while continuing, at most 8 continuations in a row unless a tool runs).
 */
function registerStopHook(pi: ExtensionAPI, session: HookSession, dispatch: HookRunner['dispatch']): void {
  /** Stop-hook continuations in a row (reset by a prompt or a tool call). */
  let stopContinues = 0;
  pi.on('before_agent_start', async () => {
    stopContinues = 0;
    return undefined;
  });
  pi.on('tool_execution_start', async (event) => {
    if (!event.parentToolCallId) stopContinues = 0;
  });
  pi.on('agent_before_settle', async (event, ctx) => {
    const stops = commandsFor(session.config, 'Stop', []);
    if (event.outcome !== 'completed' || stops.length === 0) return undefined;
    const last = [...event.context.llmMessages].reverse().find((message) => message.role === 'assistant');
    // Pi removed its working indicator at `agent_end`; the footer status shows the hooks still running.
    const verdict = await withStatus(ctx, hooksLabel('Stop', stops), () => dispatch(ctx, 'Stop', [], () => ({ stop_hook_active: stopContinues > 0, last_assistant_message: last ? contentText(last.content) : '' })));
    if (!verdict.block || stopContinues >= STOP_CONTINUE_CAP) return undefined;
    stopContinues += 1;
    // Pi replaces the boundary's entries with a handler's: keep what earlier handlers proposed.
    return { entries: [...event.entries, { type: 'custom_message' as const, customType: STOP_TYPE, content: `${STOP_PREFIX}${verdict.block}`, display: true }], continue: true };
  });
}

/**
 * Optional adapter for Claude Code / Codex command hooks, adapted from the native host's hook dispatcher:
 * PreToolUse → `tool_call` (can block), PostToolUse → `tool_result` (can add feedback), SessionStart (also after a
 * compaction, source `compact`) → context for the next prompt, PreCompact → `session_before_compact` (side effects
 * only), Stop → `agent_before_settle` (a block makes the agent continue with the reason), Notification → when Pi asks
 * the user something or an answer waits (side effects only). Off unless `OCTOCODE_HOOKS=1`, because hooks written for
 * another agent run arbitrary commands. Octocode's PreMessageSend gates team/API/external sends; PostMessageReceive
 * observes actual delivery without changing the body or requesting a reply.
 */
export function registerHooks(pi: ExtensionAPI, env: NodeJS.ProcessEnv = process.env, home?: string, onFailure?: HookFailureSink): HooksView {
  const session: HookSession = { config: emptyHooks(), errors: [], root: undefined };
  const enabled = hooksEnabled(env);
  const timings = new Map<string, HookTiming>();
  const view: HooksView = {
    summary: () => (enabled ? `on, ${hookCount(session.config)} commands` : `off (${HOOKS_ENV}=1 to enable)`),
    report: (cwd) => hooksReport(enabled, session.config, session.errors, cwd, home, timings),
    preToolUse: async () => undefined,
    notification: () => undefined,
    messageSend: async () => undefined,
    messageReceive: async () => undefined,
  };
  // Registered even when hooks are off: a resumed session may hold Stop feedback from an earlier run.
  registerStopRenderer(pi);
  if (!enabled) return view;
  const runner = createDispatcher(session, timings, onFailure);
  // The message handlers go first: their session_start reset runs before the hook files load.
  Object.assign(view, registerMessageHooks(pi, runner));
  registerStopHook(pi, session, runner.dispatch);
  registerSessionStartHooks(pi, session, runner, home);
  view.preToolUse = preToolUseGate(runner.dispatch);
  view.notification = notificationHook(runner);
  registerPostToolUseHook(pi, runner.dispatch);
  registerPreCompactHook(pi, runner.dispatch);
  return view;
}

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

/** `/octocode hooks` (or `/hooks`): whether hooks run, what loaded, and which files to edit. */
export function registerHooksCommand(commands: Subcommands, hooks: HooksView): void {
  commands.add('hooks', {
    description: 'hooks — hooks on/off, loaded commands and the files to configure',
    handler: async (_args, ctx) => say(ctx, hooks.report(ctx.cwd)),
  });
}
