import path from 'node:path';
import { StringEnum } from '@earendil-works/pi-ai';
import type { ExtensionAPI, ExtensionContext, ToolCallEvent, ToolCallEventResult } from '@earendil-works/pi-coding-agent';
import { Box, Text } from '@earendil-works/pi-tui';
import { Type } from 'typebox';
import { contentText, firstLine, isRecord, modelLookup, textResult, unpricedModel } from '../shared/util.js';
import { storedText } from '../shared/sanitize.js';
import { formatClock } from '../shared/format.js';
import { resolveToolPath } from '../shared/home.js';
import { clip, plural, resultBlock, toolHeader } from '../shared/render.js';
import { timingOf, timedTool } from '../shared/tool-timing.js';
import { EDIT_RETENTION_MINUTES } from './edits.js';
import { describeChanges, describeConflicts, describeMembers, mutatedPaths, senderLabel } from './routing.js';
import { checkGoal, GOAL_DESCRIPTION } from './identity.js';
import type { Team } from './session.js';
import { INTERNAL_MESSAGE_TYPE } from '../shared/messages.js';
import type { MailDetails } from './model.js';

const DEFAULT_CHANGES_WINDOW_MIN = 60;
const DEFAULT_CHANGES_LIMIT = 20;

type StopAgent = (id: string, by: string) => string | undefined;

/** Every field the `coordinate` schema accepts: a subagent's schema has no `id` or `goal` (`stop` and `join` are refused there before they are read). */
interface CoordinateParams {
  action: 'list' | 'join' | 'leave' | 'lock' | 'unlock' | 'stop' | 'changes';
  id?: string;
  goal?: string;
  paths?: string[];
  withinMinutes?: number;
  limit?: number;
  reason?: string;
}

/**
 * Registers the team tools and events; returns the reservation gate for the tool-call pipeline (`index.ts`).
 * `stopAgent` stops a background subagent this session started (wired by `index.ts` from the subagents domain); unset
 * in a subagent, which cannot start any. `beforeLeave` runs at session shutdown before this agent leaves the team:
 * the subagents it stops can still message it and release their reservations while it is a member.
 */
export function registerCollab(
  pi: ExtensionAPI,
  team: Team,
  options: { stopAgent?: StopAgent; beforeLeave?: (reason: string) => Promise<unknown> } = {},
): { reservationGate: (event: ToolCallEvent, ctx: ExtensionContext) => Promise<ToolCallEventResult | undefined> } {
  registerMessageRenderer(pi);
  registerCoordinateTool(pi, team, options.stopAgent);
  registerSendMessageTool(pi, team);
  registerTeamEvents(pi, team, options.beforeLeave);
  // Reservations are enforced where the edit happens, so a model that forgot to check still cannot overwrite a peer.
  // The `file` tool checks each of its queries itself (one held path fails one change, not the whole batch); this covers Pi's own edit and write.
  const reservationGate = async (event: ToolCallEvent, ctx: ExtensionContext): Promise<ToolCallEventResult | undefined> => {
    if (event.toolName === 'file') return undefined;
    for (const file of mutatedPaths(event.toolName, event.input)) {
      const reason = team.reservation(resolveToolPath(ctx.cwd, file), file);
      if (reason) return { block: true, reason };
    }
    return undefined;
  };
  return { reservationGate };
}

function registerMessageRenderer(pi: ExtensionAPI): void {
  // Agent messages are shown in full, collapsed or not: they are short (a long one arrives as its start plus a file path) and the user reads them.
  pi.registerMessageRenderer(INTERNAL_MESSAGE_TYPE, (message, { outputPad }, theme) => {
    const mail = (message.details as Partial<MailDetails> | undefined)?.messages;
    const box = new Box(outputPad ?? 1, 0);
    // Details are another process's data: anything but a batch shows the text the model got.
    const entries = Array.isArray(mail) ? mail : [{ from: 'agent', text: typeof message.content === 'string' ? message.content : '', at: Date.now() }];
    for (const entry of entries) {
      const sender = entry?.sender;
      const from = clip(`${String(entry?.from ?? 'agent')}${sender ? ` (${senderLabel(sender)})` : ''}`, 100);
      const kind = entry?.replyRequired ? ' · reply requested' : '';
      box.addChild(new Text(`${theme.fg('accent', '✉')} ${theme.fg('toolTitle', theme.bold(`from ${from}`))} ${theme.fg('dim', `· ${formatClock(entry?.at ?? Date.now())}${kind}`)}`, 0, 0));
      // resultBlock sanitizes the body.
      box.addChild(resultBlock(theme, { expanded: true }, { summary: '', body: String(entry?.text ?? ''), error: false }));
    }
    return box;
  });
}

function registerCoordinateTool(pi: ExtensionAPI, team: Team, stopAgent: StopAgent | undefined): void {
  // `stop` exists only for an agent that can start subagents, which is also the one that joins and leaves by hand: a
  // subagent joins when it starts and leaves when its run ends (`Team.start`, `onAgentSettled`), and would only be refused `stop`.
  const canStop = stopAgent !== undefined;
  const actions = canStop ? (['list', 'join', 'leave', 'lock', 'unlock', 'stop', 'changes'] as const) : (['list', 'lock', 'unlock', 'changes'] as const);
  pi.registerTool(timedTool({
    name: 'coordinate',
    label: 'Coordinate',
    description:
      'See which agents work in this repository and reserve the files you will change, so concurrent edits do not collide. Reservations are cooperative: they cover guarded file tools, not shell commands, last while you work, and make a conflicting edit fail.',
    promptSnippet: 'List agents, reserve files, see recent changes',
    promptGuidelines: ['Run `list` before work that may overlap another agent, `lock` the paths you will edit and `unlock` them when done. On a conflict, message the owner or do independent work.'],
    parameters: Type.Object({
      action: StringEnum(actions, {
        description:
          'list: live agents, their activity and held paths; ' + (canStop ? 'join: register or update your goal; leave: unregister and release your reservations; ' : '') +
          'lock: reserve paths, all or none; unlock: release the given paths, or all without paths; ' +
          (canStop ? 'stop: end a background subagent you started (id), killing its process tree; its report says it was stopped; ' : '') +
          'changes: files agents changed recently, newest first, with who and when',
      }),
      ...(canStop ? { id: Type.Optional(Type.String({ description: 'stop: id of the background subagent to end' })) } : {}),
      ...(canStop ? { goal: Type.Optional(Type.String({ description: `join: your goal. ${GOAL_DESCRIPTION}` })) } : {}),
      paths: Type.Optional(Type.Array(Type.String(), { description: 'lock/unlock: files, or "dir/" for a tree; unlock without paths releases all. changes: only files under these paths' })),
      withinMinutes: Type.Optional(Type.Integer({ description: 'changes: how far back to look (default 60)' })),
      limit: Type.Optional(Type.Integer({ description: 'changes: most files to list (default 20)' })),
      reason: Type.Optional(Type.String({ description: 'lock: what you will change there' })),
    }),
    async execute(_id, input, signal, _onUpdate, ctx) {
      // The schema's optional `id` and `goal` depend on the agent, so Pi infers them as unknown: this is the one place that names them.
      const params = input as CoordinateParams;
      signal?.throwIfAborted();
      const now = Date.now();
      if (params.action === 'join') team.join(params.goal ? checkGoal(params.goal) : undefined);
      else if (params.action === 'leave') {
        team.leave();
        return textResult('Left the team.');
      } else if (params.action === 'lock') {
        if (!params.paths?.length) throw new Error('lock needs paths.');
        const result = team.lock(params.paths, storedText(params.reason, 200) || 'editing');
        if (!result.ok) throw new Error(describeConflicts(result.conflicts, now));
        const total = result.held > result.leases.length ? ` You now hold ${plural(result.held, 'reservation')}.` : '';
        return textResult(`Reserved: ${result.leases.map((lease) => (lease.kind === 'tree' ? `${lease.path}/` : lease.path)).join(', ')}.${total} Unlock when your edits are done.`, { summary: `Reserved ${plural(result.leases.length, 'path')}` });
      } else if (params.action === 'unlock') {
        const released = team.unlock(params.paths);
        return textResult(`Released ${released} reservation(s).`, { summary: `Released ${plural(released, 'reservation')}` });
      } else if (params.action === 'stop') {
        if (!stopAgent) throw new Error('Only the agent that started a subagent can stop it, and you have started none.');
        const id = params.id;
        const refused = stopAgent(id?.trim() ?? '', team.id ?? 'the parent agent');
        if (refused) throw new Error(refused);
        return textResult(`Stopping ${id}: its process tree is killed and its report arrives as a message.`);
      } else if (params.action === 'changes') {
        const requested = Math.max(1, Math.floor(params.withinMinutes ?? DEFAULT_CHANGES_WINDOW_MIN));
        // Changes older than the retention are gone: search what is kept and say so (describeChanges).
        const minutes = Math.min(requested, EDIT_RETENTION_MINUTES);
        const log = team.editLog();
        const prefixes = log && params.paths?.map((entry) => path.relative(log.workspace, resolveToolPath(ctx.cwd, entry)).split(path.sep).join('/'));
        const changes = log?.changes({ withinMs: minutes * 60_000, ...(prefixes?.length ? { prefixes } : {}) }) ?? [];
        const text = describeChanges(changes, { self: team.id, minutes, requested, limit: Math.max(1, Math.floor(params.limit ?? DEFAULT_CHANGES_LIMIT)), paths: params.paths }, now);
        return textResult(text, { summary: changes.length ? `${plural(changes.length, 'file')} changed in ${minutes}m` : `No changes in ${minutes}m` });
      }
      const { members, error } = team.snapshot();
      if (error) throw new Error(error);
      const working = members.filter((member) => member.status === 'working').length;
      return textResult(describeMembers(members, team.id, now), { summary: members.length ? `${plural(members.length, 'agent')} · ${working} working` : 'No agents' });
    },
    renderCall(args, theme, context) {
      const paths = Array.isArray(args.paths) ? args.paths.map(String) : [];
      const target = paths.length > 1 ? plural(paths.length, 'path') : (paths[0] ?? '');
      const id = (args as Partial<CoordinateParams>).id;
      const summary = [String(args.action ?? ''), id === undefined ? '' : String(id), target, args.goal ? `"${String(args.goal)}"` : ''].filter(Boolean).join(' ');
      return toolHeader(theme, context, 'Coordinate', summary);
    },
    renderResult(result, _options, theme, context) {
      const body = contentText(result.content);
      const summary = isRecord(result.details) && typeof result.details['summary'] === 'string' ? result.details['summary'] : firstLine(body);
      // The agent list and change log stay folded: the summary counts them and ctrl+o shows them.
      return resultBlock(theme, context, { summary, body: summary === firstLine(body) ? body.split('\n').slice(1).join('\n') : body, max: context.isError ? 3 : 0, ...timingOf(result.details) });
    },
  }));
}

function registerSendMessageTool(pi: ExtensionAPI, team: Team): void {
  pi.registerTool(timedTool({
    name: 'sendMessage',
    label: 'Send message',
    description:
      'Send steering, a question or evidence to a live agent in this repository, or to every agent in your session tree. Delivery is queued: a busy recipient reads it at its next step, and you are told if it left unread. ' +
      'A finished subagent that lingers (its report says so, and for how long) resumes with your message; one that does not takes no more messages, so give it a new `agent` task. A long message arrives as its start plus the path of a file with the full text.',
    promptSnippet: 'Message a subagent or teammate',
    promptGuidelines: ['Message only when it changes the recipient\'s next step: a blocker (SBAR), a decision request, or evidence as a delta update. Make it self-contained and skip acknowledgements and completion pings (final reports arrive automatically).'],
    parameters: Type.Object({
      to: Type.Optional(Type.String({ description: 'Agent id, role, "parent" or "all" (your session tree). Omit it when replyTo is set: the message goes to the sender of that message' })),
      message: Type.String({ description: 'The message text' }),
      replyRequired: Type.Optional(Type.Boolean({ description: 'true: a request (a question or task you need answered); wakes an idle recipient. false (the default): an FYI, no reply needed' })),
      replyTo: Type.Optional(Type.Integer({ description: 'Number of the message you answer, to reply in its thread (no `to` needed); answering a question wakes the asker' })),
      interrupt: Type.Optional(Type.Boolean({ description: 'true: stop the current step of one subagent you started (its running tool is cancelled) and continue it with this message, for a changed plan or wrong direction. Default false: read at its next step' })),
    }),
    async execute(_id, params, signal) {
      signal?.throwIfAborted();
      team.join();
      const result = await team.send(params.to, params.message, {
        ...(signal ? { signal } : {}),
        ...(params.replyRequired !== undefined ? { replyRequired: params.replyRequired } : {}),
        ...(params.replyTo !== undefined ? { replyTo: params.replyTo } : {}),
        ...(params.interrupt ? { interrupt: true } : {}),
      });
      if ('error' in result) throw new Error(result.error);
      if (result.resumed) return textResult(result.resumed, { to: result.sent });
      if (params.interrupt) return textResult(`Interrupt queued for ${result.sent.join(', ')} as message #${result.id}: a busy subagent stops its current step within a second.`, { to: result.sent, id: result.id });
      const departed = result.departed?.length ? ` Not sent to ${result.departed.join(', ')} (left the team).` : '';
      const recipients = result.sent.join(', ');
      const mode = params.replyRequired === true
        ? `as a request: ${recipients} ${result.sent.length > 1 ? 'are' : 'is'} woken and must answer with replyTo ${result.id}`
        : 'as an FYI (no reply requested)';
      return textResult(`Queued message #${result.id} for ${recipients} ${mode}.${departed}`, { to: result.sent, id: result.id, ...(result.departed?.length ? { departed: result.departed } : {}) });
    },
    renderCall(args, theme, context) {
      const to = args.to === undefined ? '' : `→ ${String(args.to)}`;
      const reply = args.replyTo === undefined ? '' : `reply to #${String(args.replyTo)}`;
      return toolHeader(theme, context, 'Send', [to, reply].filter(Boolean).join(' · '));
    },
    renderResult(result, _options, theme, context) {
      const details: Record<string, unknown> = isRecord(result.details) ? result.details : {};
      const text = contentText(result.content);
      const sent = Array.isArray(details['to']) ? details['to'].map(String) : [];
      const departed = Array.isArray(details['departed']) && details['departed'].length ? ` · not sent to ${details['departed'].map(String).join(', ')}` : '';
      const summary = typeof details['id'] === 'number' ? `Queued #${details['id']}${sent.length > 1 ? ` to ${plural(sent.length, 'agent')}` : ''}${departed}` : firstLine(text);
      // The message itself is the model's own words: shown only when expanded.
      const message = isRecord(context.args) ? String(context.args['message'] ?? '') : '';
      return resultBlock(theme, context, { summary, body: context.isError ? text.split('\n').slice(1).join('\n') : message, max: context.isError ? 3 : 0, ...timingOf(result.details) });
    },
  }));
}

function registerTeamEvents(pi: ExtensionAPI, team: Team, beforeLeave?: (reason: string) => Promise<unknown>): void {
  pi.on('session_start', async (_event, ctx) => team.start(ctx));
  pi.on('session_shutdown', async (event) => {
    try {
      // The shutdown's reason (`reload`, `quit`…) names what stopped the subagents in their reports.
      await beforeLeave?.(event.reason);
    } finally {
      team.stop();
    }
  });
  pi.on('agent_start', async () => team.onAgentStart());
  pi.on('agent_settled', async () => team.onAgentSettled());
  // Team mail is delivered when the model is called with it. Pi emits `context` before every model call, whichever way
  // a message entered the conversation (message_start reaches extensions only for steered and prompted messages).
  pi.on('context', async (event) => void team.onContext(event.messages as Array<{ role?: string; customType?: string; details?: unknown }>));
  // Who changed which file, for `coordinate changes`: Pi's edit/write by their arguments, `file` by its outcomes (a
  // batch can partly fail). Arguments are only on the start event, so they wait here for the end.
  const pending = new Map<string, string[]>();
  pi.on('tool_execution_start', async (event) => {
    if (!event.parentToolCallId) team.onToolStart();
    if (event.toolName === 'edit' || event.toolName === 'write') pending.set(event.toolCallId, mutatedPaths(event.toolName, event.args));
  });
  pi.on('tool_execution_end', async (event, ctx) => {
    const args = pending.get(event.toolCallId);
    pending.delete(event.toolCallId);
    if (event.isError) return;
    const outcomes = event.toolName === 'file' && isRecord(event.result) && isRecord(event.result['details']) ? event.result['details']['outcomes'] : undefined;
    const files = Array.isArray(outcomes) ? outcomes.flatMap((outcome) => (isRecord(outcome) && outcome['ok'] === true && typeof outcome['path'] === 'string' ? [outcome['path']] : [])) : (args ?? []);
    if (files.length === 0) return;
    try {
      team.editLog()?.record(team.id ?? `pid ${process.pid}`, event.toolName, files.map((file) => resolveToolPath(ctx.cwd, file)));
    } catch {
      // A missing record only makes `coordinate changes` less complete.
    }
  });
  pi.on('message_end', async (event, ctx) => {
    if (event.message.role === 'assistant') team.onUsage(event.message.usage, event.message.model, unpricedModel(event.message, modelLookup(ctx)) !== undefined);
  });
}
