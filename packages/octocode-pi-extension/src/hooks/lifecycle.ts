import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { STATUS } from '../shared/status.js';
import { notify } from '../shared/commands.js';
import { projectTrust } from '../shared/trust.js';
import { contentText, isRecord } from '../shared/util.js';
import { hookCalls, loadHooks } from './config.js';
import { capContext } from './runner.js';
import type { HookRunner, HookSession } from './types.js';

const CONTEXT_TYPE = 'octocode-hook-context';
/** Longest the first prompt waits for SessionStart hooks still running. */
const START_HOOKS_WAIT_MS = 15_000;

/** Pi's session start reasons in Claude Code's SessionStart matcher vocabulary. */
const START_SOURCE: Record<string, string> = { startup: 'startup', reload: 'startup', new: 'clear', resume: 'resume', fork: 'resume' };

/**
 * SessionStart (also after a compaction, source `compact`): loads the hook config on each session start, runs the
 * SessionStart hooks, and hands their context to the next prompt.
 */
export function registerSessionStartHooks(pi: ExtensionAPI, session: HookSession, { dispatch, reportFailure }: HookRunner, home: string | undefined): void {
  let pending: string[] = [];
  /** SessionStart hook commands still running: the next prompt waits for their context, session start does not. */
  let starting: Promise<void> | undefined;
  /** Ends the current session's SessionStart hooks (on shutdown or the next session start), so none outlive it. */
  let startAbort = new AbortController();

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
        if (!abort.signal.aborted) reportFailure(ctx, 'SessionStart', error);
      });
    const run: Promise<void> = (previous ? Promise.all([previous, hooks]) : hooks).then(() => undefined).finally(() => {
      if (starting === run) starting = undefined;
    });
    starting = run;
  };

  pi.on('session_start', async (event, ctx) => {
    const loaded = loadHooks(ctx.cwd, await projectTrust(ctx), home);
    session.config = loaded.config;
    session.errors = loaded.errors;
    session.root = undefined;
    pending = [];
    if (loaded.errors.length > 0 && ctx.hasUI) notify(ctx.ui, `Ignoring invalid hook config: ${loaded.errors.join('; ')}`, 'warning');
    startHooks(ctx, START_SOURCE[event.reason] ?? 'startup', true);
  });
  // Claude Code runs SessionStart again (source `compact`) after a compaction: its context comes with the next prompt.
  pi.on('session_compact', async (_event, ctx) => startHooks(ctx, 'compact', false));
  pi.on('session_shutdown', async () => startAbort.abort());

  // SessionStart context reaches the model once, with the next prompt, as data rather than instructions.
  pi.on('before_agent_start', async (_event, ctx) => {
    // The first prompt waits a bounded time for their context; a slower hook's context comes with the next prompt.
    if (starting) {
      // Shown only while the prompt actually waits: `starting` clears itself once the hooks finish.
      if (ctx.hasUI) ctx.ui.setStatus(STATUS.hooks, 'Waiting for SessionStart hooks…');
      try {
        await Promise.race([starting, new Promise((resolve) => setTimeout(resolve, START_HOOKS_WAIT_MS).unref())]);
      } finally {
        if (ctx.hasUI) ctx.ui.setStatus(STATUS.hooks, undefined);
      }
    }
    if (pending.length === 0) return undefined;
    const content = capContext(`Context from SessionStart hooks:\n${pending.join('\n\n')}`);
    pending = [];
    return { message: { customType: CONTEXT_TYPE, content, display: false } };
  });
}

/** PostToolUse: feedback from the hooks is appended to the tool result the model reads. */
export function registerPostToolUseHook(pi: ExtensionAPI, dispatch: HookRunner['dispatch']): void {
  pi.on('tool_result', async (event, ctx) => {
    if (event.isError) return undefined;
    const outcomes = event.toolName === 'file' && isRecord(event.details) && Array.isArray(event.details['outcomes']) ? event.details['outcomes'] : undefined;
    const feedback: string[] = [];
    for (const [index, call] of hookCalls(event.toolName, event.input, ctx.cwd).entries()) {
      // A file batch may succeed only in part. Success hooks must not touch rejected or failed queries.
      if (outcomes && (!isRecord(outcomes[index]) || outcomes[index]['ok'] !== true)) continue;
      const verdict = await dispatch(ctx, 'PostToolUse', call.names, () => ({ tool_name: call.tool_name, pi_tool_name: event.toolName, tool_input: call.tool_input, tool_response: contentText(event.content) }));
      feedback.push(...[verdict.block, verdict.context].filter((text): text is string => Boolean(text)));
    }
    return feedback.length > 0 ? { content: [...event.content, { type: 'text' as const, text: `\n[PostToolUse hook] ${capContext(feedback.join('\n\n'))}` }] } : undefined;
  });
}

/** PreCompact: side effects only. */
export function registerPreCompactHook(pi: ExtensionAPI, dispatch: HookRunner['dispatch']): void {
  pi.on('session_before_compact', async (event, ctx) => {
    const trigger = event.reason === 'manual' ? 'manual' : 'auto';
    await dispatch(ctx, 'PreCompact', [trigger], () => ({ trigger, custom_instructions: event.customInstructions ?? '' }));
    return undefined;
  });
}
