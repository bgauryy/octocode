import { VERSION, type ExtensionAPI, type ExtensionContext, type ToolCallEvent } from '@earendil-works/pi-coding-agent';
import { minimumPiVersion, versionBelow } from './shared/package.js';
import { registerApi } from './api/register.js';
import { registerExternal } from './external/register.js';
import { externalPermissionDialog } from './external/permission-dialog.js';
import type { ExternalAgents } from './external/manager.js';
import { EXTERNAL_SENDER, type MessageEnvelope } from './shared/messages.js';
import { registerAskUser } from './ask/tool.js';
import { registerArchiveCommand } from './compaction/archive-command.js';
import { registerArchiveTool } from './compaction/archive-tool.js';
import { registerCompaction } from './compaction/register.js';
import { registerReadDedupe } from './files/dedupe.js';
import { registerBashTool } from './files/bash.js';
import { Checkpoints } from './files/checkpoint.js';
import { registerCheckpoints } from './files/checkpoint-command.js';
import { registerHooks, registerHooksCommand } from './hooks/register.js';
import { contextReads } from './files/reads.js';
import { FileGuard } from './files/guard.js';
import { registerFileTool } from './files/tool.js';
import { registerMcpCommand } from './mcp/command.js';
import { registerOctocodeMcp } from './mcp/octocode.js';
import { envFlag, SUBAGENT_ENV } from './shared/env.js';
import { Permission } from './permission/ask.js';
import { registerPermissionCommand } from './permission/command.js';
import { permissionGate, type GateVerdict } from './permission/gate.js';
import { PROFILE_DECLARABLE } from './permission/mode.js';
import { createProvenance } from './permission/provenance.js';
import { pruneSnapshots } from './permission/snapshot.js';
import { userBashGate } from './permission/user-bash.js';
import { notify, Subcommands } from './shared/commands.js';
import { projectTrust, registerTrustCommand } from './shared/trust.js';
import { errorMessage, firstLine, MINUTE_MS, settleWithin } from './shared/util.js';
import { plural } from './shared/render.js';
import { stopRowTimers } from './shared/tool-timing.js';
import { forgetCheck, recordCheck, withDialog } from './shared/locks.js';
import { registerSkills } from './skills.js';
import { closeSharedAgentDb } from './agentdb/db.js';
import { registerSessions } from './sessions/register.js';
import { registerBacklog } from './backlog/index.js';
import { registerMemory } from './memory/register.js';
import { setCurrentSession } from './shared/home.js';
import { currentCatalog } from './subagents/catalog.js';
import { registerModelsCommand } from './subagents/models-command.js';
import { loadProfiles, profileFileName, type AgentProfile } from './subagents/profiles.js';
import { maxSubagents } from './subagents/claim.js';
import { registerAgentTool, type AgentControl } from './subagents/tool.js';
import { Team } from './team/session.js';
import { spilledMessage } from './team/spill.js';
import { registerCollab } from './team/tools.js';
import { registerTurnSetup } from './turn.js';
import { registerActivity } from './ui/activity.js';
import { registerEditor } from './ui/editor.js';
import { registerNotify } from './ui/notify.js';
import { packageVersion, registerUi } from './ui/chrome.js';
import { Delivery } from './ui/delivery.js';
import { BrowserTool, registerBrowserTool } from './browser/tool.js';
import { registerWebTool } from './web/web.js';
import { interactive } from './shared/interactive.js';
import { registerStaleBundleWarning } from './shared/stale-bundle.js';
import { windowsTree } from './shared/winhelper.js';

/**
 * Octocode for Pi. Everything here builds on Pi's own tools, skills, sessions, MCP
 * and compaction; Octocode adds guarded file edits, the Octocode MCP server (its own
 * MCP client), subagents, web and browser tools, askUser and a focused prompt.
 *
 * Cross-cutting runtime policy is composed here, in an order that reads top to bottom: the `tool_call` gate pipeline
 * (invariants, hooks, then the permission mode; every refusal and call is audited; no domain registers its own
 * `tool_call` handler), the hold that keeps a headless run open for its background work, and the confirmation before a
 * session switch or fork stops that work. Domains supply the pieces (a gate, a `settle()`, a count). Pi runs an event's
 * handlers in registration order, and two events also have domain handlers whose order matters:
 * - `session_before_fork`: the leave confirmation here runs before the checkpoint file-restore offer
 *   (`files/checkpoint-command`), because it is registered before `registerCheckpoints`.
 * - `agent_before_settle`: the Stop hook continuation (`hooks/register`) and the too-large retry
 *   (`compaction/register`) are registered before the headless hold at the end of this function, so a continued or
 *   retried run is decided before the hold waits for background work.
 *
 * Every user command is a subcommand of `/octocode` (`agents`, `hooks`, `api`, `permission`, `rewind`, `jobs`, …) and is
 * listed by `/octocode help`. The frequent ones also get a top-level shortcut, all aliased here from `SHORTCUTS` (only
 * those registered in this mode); `/mcp` is Pi's built-in MCP for the user's own servers, `/octocode mcp` the Octocode one.
 */
/** Subcommands that also get a top-level `/<name>` shortcut, when this mode registers them. */
const SHORTCUTS = ['agents', 'backlog', 'hooks', 'memory', 'sessions'];

export default function octocode(pi: ExtensionAPI): void {
  const isSubagent = envFlag(process.env, SUBAGENT_ENV);
  // First, so every feature's pi.sendMessage goes through it and its agent_settled handler runs before theirs.
  Delivery.install(pi);
  const commands = new Subcommands();
  const guard = new FileGuard();
  const team = new Team(pi);
  let profiles = new Map<string, AgentProfile>();
  /** Why each profile file was refused at load (`loadProfiles`' `onSkip`), by file name: the `agent` tool names it for an unknown profile. */
  let refused = new Map<string, string>();
  const octocodeMcp = registerOctocodeMcp(pi, packageVersion());
  registerMcpCommand(commands, octocodeMcp);
  // One permission state per session: the mode, its grants, and the provenance tripwire's index.
  const permission = new Permission({ pi, isSubagent, agent: () => team.id });
  const annotations = (name: string) => pi.getAllTools().find((tool) => tool.name === name)?.annotations;
  const provenance = createProvenance({ annotations });
  registerStaleBundleWarning(pi, { isSubagent });

  pi.on('session_start', async (_event, ctx) => {
    // Before anything writes output: spilled results, bash logs and checkpoints go to this session's folder.
    setCurrentSession(ctx.sessionManager.getSessionId());
    // Every start (resume and fork included) re-applies the start mode, so `full` never comes back silently.
    permission.start(ctx);
    provenance.clear();
    // Restore points older than a week go; off the start path, and never an error for the session.
    void pruneSnapshots(ctx.cwd);
    // An older Pi misses fixes Octocode relies on (Pi 1.1.0 dropped extension prompt sections on runs a background
    // report started, rewriting the whole prompt cache twice per report).
    const minimum = minimumPiVersion();
    if (minimum && versionBelow(VERSION, minimum) && interactive(ctx, isSubagent)) {
      notify(ctx.ui, `Pi ${VERSION} is older than the ${minimum} Octocode needs: prompt caching and other features misbehave. Update: npm i -g @earendil-works/pi-coding-agent@latest`, 'warning');
    }
  });
  // Every result, nested codemode calls included, before any later call can repeat its text.
  pi.on('tool_result', async (event, ctx) => provenance.record(event, ctx));

  guard.reservedBy = (file) => team.reservation(file, file);
  const checkpoints = new Checkpoints();
  registerFileTool(pi, guard, permission, checkpoints);
  // Pi's bash with a foreground deadline and background jobs.
  const jobs = registerBashTool(pi, commands);
  let agents: AgentControl | undefined;
  let external: ExternalAgents | undefined;
  // /new, /resume and /fork end this runtime, and with it its bash jobs and background subagents: ask before leaving
  // them. Registered before the checkpoint handlers, so a fork is confirmed before its file-restore offer.
  const confirmLeave = async (ctx: ExtensionContext, action: string) => {
    if (!interactive(ctx, isSubagent)) return undefined;
    const running = [jobs.jobs.size && plural(jobs.jobs.size, 'bash job'), agents?.background() && plural(agents.background(), 'background subagent'), external?.running && plural(external.running, 'external prompt')].filter(Boolean);
    if (running.length === 0) return undefined;
    const leave = await withDialog(() => ctx.ui.confirm('Running work', `Stop ${running.join(' and ')} and ${action}?`));
    return leave ? undefined : { cancel: true };
  };
  pi.on('session_before_switch', async (event, ctx) => confirmLeave(ctx, event.reason === 'new' ? 'start a new session' : 'switch sessions'));
  pi.on('session_before_fork', async (_event, ctx) => confirmLeave(ctx, 'fork'));
  registerCheckpoints(pi, checkpoints, (file) => guard.forget(file), commands);
  registerSessions(pi, { commands, isSubagent, filesChanged: () => new Set(checkpoints.list().map((entry) => entry.path)).size });
  registerBacklog(pi, { commands, isSubagent, agentId: () => team.id });
  registerMemory(pi, { commands, isSubagent, permission });
  const skillCount = () => pi.getCommands().filter((command) => command.source === 'skill').length;
  registerWebTool(pi, permission);
  registerBrowserTool(pi, new BrowserTool(permission));
  const collab = registerCollab(pi, team, { stopAgent: isSubagent ? undefined : (id, by) => agents?.stop(id, by), beforeLeave: async (reason) => agents?.stopAll(reason) });
  if (!isSubagent) {
    registerPermissionCommand(commands, permission, () => provenance.size());
    registerEditor(pi, permission);
    registerTrustCommand(commands);
    registerAskUser(pi);
    agents = registerAgentTool(pi, () => profiles, team, permission, () => refused);
    commands.add('agents', agents.command);
    registerModelsCommand(commands, () => profiles);
  }
  registerSkills(pi);
  registerUi(pi, isSubagent);
  // What the agent does now: Pi's working line (main session) and the team panel's activity column (every agent).
  const activity = registerActivity(pi, { workingLine: !isSubagent, onChange: (line) => team.setActivity(line) });
  // Every call a permission check lets through (full mode asks nothing) shows as a short line while its tool runs.
  permission.onAction((event) => activity.consequential(event.action, event.what, event.warn));
  const turn = registerTurnSetup(pi, { isSubagent, profiles: () => profiles, octocode: octocodeMcp });
  const hooks = registerHooks(pi, process.env, undefined, (what, note, allowed) => permission.noted(what, note, { allowed, via: 'hook' }));
  const beforeMessage = async (message: MessageEnvelope, ctx: ExtensionContext, signal?: AbortSignal) => {
    const verdict = await hooks.messageSend(message, signal ? { ...ctx, signal } : ctx);
    if (verdict?.block) {
      permission.refused([`${message.from} → ${message.to}`], verdict.block, 'message');
      throw new Error(verdict.block);
    }
    if (verdict?.ask) {
      const checked = await permission.check(ctx, { title: 'Send this message?', what: [`${message.from} → ${message.to}`, message.text], reasons: ['hook.ask'], reason: verdict.ask, ...(signal ? { signal } : {}) });
      if (!checked.allowed) throw new Error(checked.message);
    }
    signal?.throwIfAborted();
  };
  team.setMessageHook(beforeMessage);
  if (!isSubagent) {
    external = registerExternal(pi, commands, {
      sender: (ctx) => team.id ?? ctx.sessionManager.getSessionId(),
      beforeSend: beforeMessage,
      afterReceive: (message, ctx) => hooks.messageReceive(message, ctx),
      authorize: async (input, ctx, signal) => {
        const checked = await permission.check(ctx, {
          action: 'agent.spawn', title: input.action === 'connect' ? 'Connect this external agent?' : 'Send this external prompt?',
          what: [JSON.stringify(input)], cwd: input.cwd ?? ctx.cwd, signal,
          effect: 'The external agent uses its own credentials, tools and permission policy.',
        });
        if (!checked.allowed) throw new Error(checked.message);
      },
      permission: externalPermissionDialog,
    });
    permission.onRevoke(() => { void external?.reset(); });
  }
  // After the hooks (handlers run in registration order): a PreCompact hook runs before Octocode adds its `file` tool's
  // paths to the file lists Pi carries into its summary. Pi writes the summary itself.
  registerCompaction(pi, guard, isSubagent);
  registerArchiveCommand(commands);
  registerArchiveTool(pi);
  // After the file guard's `tool_result` handler (`registerFileTool`), which must digest the text a read returned.
  registerReadDedupe(pi);
  registerHooksCommand(commands, hooks);
  // Subagents have no terminal of their own: their parent's row and report carry their state.
  if (!isSubagent) registerNotify(pi, { onNotice: (notice, ctx) => hooks.notification(notice, ctx) });
  for (const name of SHORTCUTS) if (commands.has(name)) commands.alias(pi, name);
  const api = isSubagent
    ? undefined
    : registerApi(pi, commands, { list: () => team.members(), tell: (to, text) => team.send(to, text, { from: EXTERNAL_SENDER }), deliverable: spilledMessage }, packageVersion, {
      messageSend: beforeMessage,
      externalCall: (input, ctx) => external!.call(input, ctx),
    });

  pi.on('session_start', async (_event, ctx) => {
    // Per-session state first: a later step that throws (API port in use) must not carry the old session's over.
    turn.reset();
    await external?.reset();
    // Reads in a resumed, reloaded or forked session's context still count; a new session starts empty.
    guard.restore(contextReads(ctx));
    const skipped: string[] = [];
    refused = new Map();
    profiles = loadProfiles(ctx.cwd, undefined, await projectTrust(ctx), process.env, (file, error) => {
      skipped.push(`${file}: ${errorMessage(error)}`);
      refused.set(profileFileName(file), errorMessage(error));
    });
    // After the profiles: the main session leaves `browser` to the profiles that carry it.
    turn.settleTools();
    if (skipped.length > 0 && interactive(ctx, isSubagent)) notify(ctx.ui, `Skipped subagent profiles that could not be read:\n${skipped.join('\n')}`, 'warning');
    const ignored = [...profiles.values()].filter((profile) => profile.ignoredPermissions?.length);
    if (ignored.length > 0 && interactive(ctx, isSubagent)) notify(ctx.ui, `Ignored permissions a subagent profile cannot declare (only ${PROFILE_DECLARABLE.join(', ')}):\n${ignored.map((profile) => `${profile.name}: ${profile.ignoredPermissions!.join(', ')}`).join('\n')}`, 'warning');
    await api?.start(ctx);
    // The models.dev index, refreshed in the background, so the first subagent of the day routes with it.
    if (!isSubagent) currentCatalog();
  });

  // The tool-call pipeline, cheapest and most certain first; the first block wins. Invariants (reservations) and user
  // hooks come first: user hooks spawn processes, so they run only for calls the reservation gate lets through; the
  // permission gate checks last (a hook's `ask` is handed to it), so the user is never asked about a call a hook would
  // deny. A refusal by an earlier gate is audited here; the permission gate audits its own. Pi runs no call of a batch
  // before every call's checks ended, so their time is shown: live on the working line, then in the row (`timed` reads
  // `recordCheck`).
  type Gate = (event: ToolCallEvent, ctx: ExtensionContext, prior?: GateVerdict) => Promise<GateVerdict | undefined>;
  const audited = (gate: Gate): Gate => async (event, ctx, prior) => {
    const verdict = await gate(event, ctx, prior);
    if (verdict?.block) permission.refused([`${event.toolName} ${JSON.stringify(event.input)}`], verdict.reason ?? 'blocked', event.toolName);
    return verdict;
  };
  const gates: Gate[] = [audited(collab.reservationGate), audited(hooks.preToolUse), permissionGate({ permission, provenance, annotations })];
  pi.on('tool_call', async (event, ctx) => {
    const started = Date.now();
    activity.checking(event.toolCallId, true);
    try {
      let prior: GateVerdict | undefined;
      for (const gate of gates) {
        const verdict = await gate(event, ctx, prior);
        if (verdict?.block) return verdict;
        if (verdict?.ask) prior = { ask: verdict.ask };
      }
      return undefined;
    } finally {
      recordCheck(event.toolCallId, Date.now() - started);
      activity.checking(event.toolCallId, false);
    }
  });
  pi.on('tool_execution_end', async (event) => forgetCheck(event.toolCallId));

  // The user's own `!cmd`: the hooks and the permission check decide, as for an agent's `bash` call (`permission/user-bash`).
  pi.on('user_bash', userBashGate({ permission, preToolUse: hooks.preToolUse }));

  // Headless runs (print, JSON, subagents) end with the agent's run, which would lose background work before it reports:
  // hold the run until bash jobs and background subagents finish (their reports arrive as one more turn). An interactive
  // session goes on; an aborted or failed run has no turn to hand the reports to.
  pi.on('agent_before_settle', async (event, ctx) => {
    if (interactive(ctx, isSubagent) || event.outcome !== 'completed') return undefined;
    // ctx.signal is unset once the model's run is over; an interrupt for this agent ends the wait too.
    const stop = ctx.signal ? AbortSignal.any([ctx.signal, team.interrupted]) : team.interrupted;
    // No time cap of its own: jobs without a timeout are stopped, timed jobs end at their timeout, and a subagent's
    // watchdog bounds it by its calls' deadlines; an interrupt ends the wait. Job and child handles are unref'd, so a
    // ref'd timer keeps the process alive until their reports arrive.
    const keepAlive = setInterval(() => undefined, MINUTE_MS);
    try {
      await settleWithin(Promise.all([jobs.settle(), agents?.settle()]), undefined, stop);
    } finally {
      clearInterval(keepAlive);
    }
    return undefined;
  });

  pi.on('session_shutdown', async (_event, ctx) => {
    permission.shutdown(ctx);
    // Rows of the old session that were still running would otherwise tick every second for the life of the process.
    stopRowTimers();
    await api?.stop();
    await external?.reset();
    // Every reason: a new, resumed, forked or reloaded session imports this module again, so this connection would never be reused.
    closeSharedAgentDb();
    // After the subagents and bash jobs (their own shutdown handlers ran first): the helper that holds their Windows job objects can end.
    await windowsTree.stopAll();
  });

  // Registered last: the command's description names every subcommand added above.
  commands.register(pi, () => {
    return [
      `Octocode ${packageVersion() ?? ''}`.trim(),
      firstLine(octocodeMcp.describe()),
      `Subagents: ${[...profiles.keys()].join(', ') || 'none'} (at most ${maxSubagents()} at once)`,
      `Skills: ${skillCount()}`,
      `Hooks: ${hooks.summary()}`,
      'Subcommands: /octocode help',
    ].join('\n');
  });
}
