import type { ExtensionAPI, ExtensionContext, ToolCallEvent } from '@earendil-works/pi-coding-agent';
import { registerApi } from './api/register.js';
import { registerAskUser } from './ask/tool.js';
import { elisionGate, registerCompaction } from './compaction/register.js';
import { bashSafetyGate, catastrophicCommand } from './files/bash-guard.js';
import { registerBashTool } from './files/bash.js';
import { Checkpoints } from './files/checkpoint.js';
import { registerCheckpoints } from './files/checkpoint-command.js';
import { REVIEW_ENV, registerReviewCommand, type ReviewMode } from './files/review.js';
import { registerHooks, registerHooksCommand } from './hooks/register.js';
import { FileGuard, registerFileTool } from './files/tool.js';
import { OCTOCODE_TOOL_PREFIX, registerOctocodeMcp } from './mcp/octocode.js';
import { envFlag, SUBAGENT_ENV } from './shared/env.js';
import { Subcommands } from './shared/commands.js';
import { projectTrust, registerTrustCommand } from './shared/trust.js';
import { settleWithin } from './shared/util.js';
import { plural, stopRowTimers } from './shared/render.js';
import { forgetCheck, recordCheck, withDialog } from './shared/locks.js';
import { registerSkills } from './skills.js';
import { closeSharedAgentDb } from './agentdb/db.js';
import { registerSessions } from './sessions/register.js';
import { registerBacklog } from './backlog/index.js';
import { registerMemory } from './memory/register.js';
import { setCurrentSession } from './shared/home.js';
import { loadProfiles, type AgentProfile } from './subagents/profiles.js';
import { maxSubagents, registerAgentTool, type AgentControl } from './subagents/tool.js';
import { EXTERNAL_SENDER, MESSAGE_MAX_CHARS, Team } from './team/session.js';
import { registerCollab } from './team/tools.js';
import { registerTurnSetup } from './turn.js';
import { registerActivity } from './ui/activity.js';
import { registerNotify } from './ui/notify.js';
import { packageVersion, registerUi } from './ui/chrome.js';
import { Delivery } from './ui/delivery.js';
import { BrowserTool, registerBrowserTool } from './browser/tool.js';
import { registerWebTool } from './web/web.js';

/**
 * Octocode for Pi. Everything here builds on Pi's own tools, skills, sessions, MCP
 * and compaction; Octocode adds guarded file edits, the Octocode MCP server (through
 * Pi's built-in MCP), subagents, web and browser tools, askUser and a focused prompt.
 *
 * Cross-cutting runtime policy is composed here, once, in an order that reads top to bottom: the `tool_call` gate
 * pipeline (also applied to the user's `!cmd` runs), the hold that keeps a headless run open for its background work,
 * and the confirmation before a session switch stops that work. Domains supply the pieces (a gate, a `settle()`, a
 * count); none registers its own handler for these events.
 *
 * Every user command is a subcommand of `/octocode` (`agents`, `hooks`, `api`, `review`, `rewind`, `jobs`, …) and is
 * listed by `/octocode help`. The frequent ones also get a top-level shortcut, all aliased here from `SHORTCUTS` (only
 * those registered in this mode); `/mcp` is Pi's built-in MCP.
 */
/** Subcommands that also get a top-level `/<name>` shortcut, when this mode registers them. */
const SHORTCUTS = ['agents', 'backlog', 'hooks', 'memory', 'sessions'];

/** Longest a headless run is held for its background work. */
const BACKGROUND_SETTLE_MS = 15 * 60_000;

export default function octocode(pi: ExtensionAPI): void {
  const isSubagent = envFlag(process.env, SUBAGENT_ENV);
  // First, so every feature's pi.sendMessage goes through it and its agent_settled handler runs before theirs.
  Delivery.install(pi);
  const commands = new Subcommands();
  const guard = new FileGuard();
  const team = new Team(pi);
  let profiles = new Map<string, AgentProfile>();
  const octocodeMcp = registerOctocodeMcp(pi);

  pi.on('session_start', async (_event, ctx) => {
    // Before anything writes output: spilled results, bash logs and checkpoints go to this session's folder.
    setCurrentSession(ctx.sessionManager.getSessionId());
  });

  guard.reservedBy = (file) => team.reservation(file, file);
  const review: ReviewMode = { on: envFlag(process.env, REVIEW_ENV) };
  const checkpoints = new Checkpoints();
  registerFileTool(pi, guard, isSubagent ? undefined : review, checkpoints);
  // Pi's bash with a foreground deadline and background jobs.
  const jobs = registerBashTool(pi, commands);
  let agents: AgentControl | undefined;
  // /new, /resume and /fork end this runtime, and with it its bash jobs and background subagents: ask before leaving
  // them. Registered before the checkpoint handlers, so a fork is confirmed before its file-restore offer.
  const confirmLeave = async (ctx: ExtensionContext, action: string) => {
    if (!ctx.hasUI) return undefined;
    const running = [jobs.jobs.size && plural(jobs.jobs.size, 'bash job'), agents?.background() && plural(agents.background(), 'background subagent')].filter(Boolean);
    if (running.length === 0) return undefined;
    const leave = await withDialog(() => ctx.ui.confirm('Running work', `Stop ${running.join(' and ')} and ${action}?`));
    return leave ? undefined : { cancel: true };
  };
  pi.on('session_before_switch', async (event, ctx) => confirmLeave(ctx, event.reason === 'new' ? 'start a new session' : 'switch sessions'));
  pi.on('session_before_fork', async (_event, ctx) => confirmLeave(ctx, 'fork'));
  registerCheckpoints(pi, checkpoints, (file) => guard.forget(file), commands);
  registerSessions(pi, { commands, isSubagent, filesChanged: () => new Set(checkpoints.list().map((entry) => entry.path)).size });
  registerBacklog(pi, { commands, isSubagent, agentId: () => team.id });
  registerMemory(pi, { commands, isSubagent });
  const skillCount = () => pi.getCommands().filter((command) => command.source === 'skill').length;
  registerWebTool(pi);
  registerBrowserTool(pi, new BrowserTool());
  const collab = registerCollab(pi, team, { stopAgent: isSubagent ? undefined : (id, by) => agents?.stop(id, by) });
  if (!isSubagent) {
    registerReviewCommand(pi, commands, review);
    registerTrustCommand(commands);
    registerAskUser(pi);
    agents = registerAgentTool(pi, () => profiles, team);
    commands.add('agents', agents.command);
  }
  registerSkills(pi);
  registerUi(pi, isSubagent);
  // What the agent does now: Pi's working line (main session) and the team panel's activity column (every agent).
  const activity = registerActivity(pi, { workingLine: !isSubagent, onChange: (line) => team.setActivity(line) });
  const turn = registerTurnSetup(pi, { isSubagent, profiles: () => profiles });
  const hooks = registerHooks(pi);
  // After the hooks (handlers run in registration order): a PreCompact hook runs before Octocode adds its `file` tool's
  // paths to the file lists Pi carries into its summary. Pi writes the summary itself.
  registerCompaction(pi, guard);
  registerHooksCommand(commands, hooks);
  // Subagents have no terminal of their own: their parent's row and report carry their state.
  if (!isSubagent) registerNotify(pi, { onNotice: (notice, ctx) => hooks.notification(notice, ctx) });
  for (const name of SHORTCUTS) if (commands.has(name)) commands.alias(pi, name);
  const api = isSubagent
    ? undefined
    : registerApi(pi, commands, { list: () => team.members(), tell: (to, text) => team.send(to, text, { from: EXTERNAL_SENDER }), maxChars: MESSAGE_MAX_CHARS }, packageVersion);

  pi.on('session_start', async (_event, ctx) => {
    profiles = loadProfiles(ctx.cwd, undefined, await projectTrust(ctx));
    turn.reset();
    await api?.start(ctx);
    guard.reset();
  });

  // The tool-call pipeline, cheapest and most certain first; the first block wins. User hooks run last: they spawn
  // processes, so only for calls the built-in gates let through. Pi runs no call of a batch before every call's checks
  // ended, so their time is shown: live on the working line, then in the row (`timed` reads `recordCheck`).
  const gates = [elisionGate, collab.reservationGate, bashSafetyGate, hooks.preToolUse];
  pi.on('tool_call', async (event, ctx) => {
    const started = Date.now();
    activity.checking(event.toolCallId, true);
    try {
      for (const gate of gates) {
        const verdict = await gate(event, ctx);
        if (verdict?.block) return verdict;
      }
      return undefined;
    } finally {
      recordCheck(event.toolCallId, Date.now() - started);
      activity.checking(event.toolCallId, false);
    }
  });
  pi.on('tool_execution_end', async (event) => forgetCheck(event.toolCallId));

  // The user's own `!cmd` runs pass the same safety gate and PreToolUse hooks as the agent's bash calls. The user may
  // still run a refused command after confirming; without a UI to confirm in, it is refused.
  const userBashChecks = [
    async (command: string) => {
      const danger = catastrophicCommand(command);
      return danger && `${danger} could destroy the system or its data.`;
    },
    async (command: string, ctx: ExtensionContext) => {
      const call = { type: 'tool_call', toolCallId: 'user-bash', toolName: 'bash', input: { command } } as ToolCallEvent;
      return (await hooks.preToolUse(call, ctx))?.reason;
    },
  ];
  pi.on('user_bash', async (event, ctx) => {
    for (const check of userBashChecks) {
      const reason = await check(event.command, ctx);
      if (!reason) continue;
      if (ctx.hasUI && (await withDialog(() => ctx.ui.confirm('Run this command?', `${reason}\n\n${event.command}\n\nRun it anyway?`)))) continue;
      return { result: { output: `Refused: ${reason}`, exitCode: undefined, cancelled: true, truncated: false } };
    }
    return undefined;
  });

  // Headless runs (print, JSON, subagents) end with the agent's run, which would lose background work before it reports:
  // hold the run until bash jobs and background subagents finish (their reports arrive as one more turn). An interactive
  // session goes on; an aborted or failed run has no turn to hand the reports to.
  pi.on('agent_before_settle', async (event, ctx) => {
    if (ctx.hasUI || event.outcome !== 'completed') return undefined;
    await settleWithin(Promise.all([jobs.settle(), agents?.settle()]), BACKGROUND_SETTLE_MS, ctx.signal);
    return undefined;
  });

  pi.on('session_shutdown', async (event) => {
    // Rows of the old session that were still running would otherwise tick every second for the life of the process.
    stopRowTimers();
    await api?.stop();
    if (event?.reason === 'quit') closeSharedAgentDb();
  });

  // Registered last: the command's description names every subcommand added above.
  commands.register(pi, () => {
    const mcpTools = pi.getAllTools().filter((tool) => tool.name.startsWith(OCTOCODE_TOOL_PREFIX)).length;
    return [
      `Octocode ${packageVersion() ?? ''}`.trim(),
      `Octocode MCP: ${octocodeMcp ? `${mcpTools} tools (see /mcp)` : 'off'}`,
      `Subagents: ${[...profiles.keys()].join(', ') || 'none'} (at most ${maxSubagents()} at once)`,
      `Skills: ${skillCount()}`,
      `Hooks: ${hooks.summary()}`,
      'Subcommands: /octocode help',
    ].join('\n');
  });
}
