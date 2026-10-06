import path from 'node:path';
import { StringEnum } from '@earendil-works/pi-ai';
import type { ExtensionAPI, ExtensionContext, ToolCallEvent, ToolCallEventResult } from '@earendil-works/pi-coding-agent';
import { Box, Text } from '@earendil-works/pi-tui';
import { Type } from 'typebox';
import { firstLine, isRecord, textResult } from '../shared/util.js';
import { storedText } from '../shared/sanitize.js';
import { formatClock } from '../shared/format.js';
import { resolveToolPath } from '../shared/home.js';
import { clip, timingOf, plural, resultBlock, timedTool, toolHeader } from '../shared/render.js';
import { describeChanges, describeConflicts, describeMembers, mutatedPaths } from './routing.js';
import { MESSAGE_MAX_CHARS, MESSAGE_TYPE, type Team } from './session.js';
import type { Message } from './model.js';

const DEFAULT_CHANGES_WINDOW_MIN = 60;
const DEFAULT_CHANGES_LIMIT = 20;

type StopAgent = (id: string, by: string) => string | undefined;

/**
 * Registers the team tools and events; returns the reservation gate for the tool-call pipeline (`index.ts`).
 * `stopAgent` stops a background subagent this session started (wired by `index.ts` from the subagents domain); unset
 * in a subagent, which cannot start any.
 */
export function registerCollab(
  pi: ExtensionAPI,
  team: Team,
  options: { stopAgent?: StopAgent } = {},
): { reservationGate: (event: ToolCallEvent, ctx: ExtensionContext) => Promise<ToolCallEventResult | undefined> } {
  registerMessageRenderer(pi);
  registerCoordinateTool(pi, team, options.stopAgent);
  registerSendMessageTool(pi, team);
  registerTeamEvents(pi, team);
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
  // Agent messages are shown in full, collapsed or not: they are short (capped at MESSAGE_MAX_CHARS) and the user reads them.
  pi.registerMessageRenderer(MESSAGE_TYPE, (message, { outputPad }, theme) => {
    const details = message.details as Partial<Message> | undefined;
    const from = clip(String(details?.from ?? 'agent'), 60);
    const kind = details?.replyRequired ? ' · reply requested' : '';
    const box = new Box(outputPad ?? 1, 0);
    box.addChild(new Text(`${theme.fg('accent', '✉')} ${theme.fg('toolTitle', theme.bold(`from ${from}`))} ${theme.fg('dim', `· ${formatClock(details?.at ?? Date.now())}${kind}`)}`, 0, 0));
    // resultBlock sanitizes the body.
    box.addChild(resultBlock(theme, { expanded: true }, { summary: '', body: String(details?.text ?? ''), error: false }));
    return box;
  });
}

function registerCoordinateTool(pi: ExtensionAPI, team: Team, stopAgent: StopAgent | undefined): void {
  // `stop` exists only for an agent that can start subagents; a subagent would only ever be refused.
  const canStop = stopAgent !== undefined;
  const actions = canStop ? (['list', 'join', 'leave', 'lock', 'unlock', 'stop', 'changes'] as const) : (['list', 'join', 'leave', 'lock', 'unlock', 'changes'] as const);
  pi.registerTool(timedTool({
    name: 'coordinate',
    label: 'Coordinate',
    description:
      'Inspect repository teammates and manage cooperative file reservations. `list`: live agents, activity and held paths. `join`: register or update your task note (subagents already join automatically); `leave`: unregister and release reservations. ' +
      '`lock`: reserve paths, all or none; `unlock`: release specified paths or all your reservations. Conflicting edits are refused. Reservations cover guarded file tools, not arbitrary shell commands; they last while you work, end when a subagent finishes, lapse after 30 min idle in a session, and within 90 s of a crash. ' +
      (canStop ? '`stop`: end a background subagent you started (`id`), killing its process tree; its report says it was stopped. ' : '') +
      '`changes`: files agents changed recently (file/edit/write), newest first with who and how long ago; narrow with `paths`, `withinMinutes` (default 60) and `limit` (default 20).',
    promptSnippet: 'Inspect teammates, reserve shared files or review recent changes',
    promptGuidelines: ['Check membership when it changes your next action. Reserve overlapping shared paths before editing and release them when done; on conflict, message the owner or do independent work.'],
    parameters: Type.Object({
      action: StringEnum(actions),
      ...(canStop ? { id: Type.Optional(Type.String({ description: 'stop: id of the background subagent to end' })) } : {}),
      note: Type.Optional(Type.String({ description: 'join: what you are working on' })),
      paths: Type.Optional(Type.Array(Type.String(), { maxItems: 32, description: 'lock/unlock: files, or "dir/" for a tree; unlock without paths releases all. changes: only files under these paths' })),
      withinMinutes: Type.Optional(Type.Integer({ minimum: 1, maximum: 1440, description: 'changes: how far back to look (default 60)' })),
      limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 100, description: 'changes: most files to list (default 20)' })),
      reason: Type.Optional(Type.String({ description: 'lock: what you will change there' })),
    }),
    async execute(_id, params, signal, _onUpdate, ctx) {
      signal?.throwIfAborted();
      const now = Date.now();
      if (params.action === 'join') team.join(params.note ? storedText(firstLine(params.note), 160) : undefined);
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
        const id = (params as { id?: string }).id;
        const refused = stopAgent(id?.trim() ?? '', team.id ?? 'the parent agent');
        if (refused) throw new Error(refused);
        return textResult(`Stopping ${id}: its process tree is killed and its report arrives as a message.`);
      } else if (params.action === 'changes') {
        const minutes = params.withinMinutes ?? DEFAULT_CHANGES_WINDOW_MIN;
        const log = team.editLog();
        const prefixes = log && params.paths?.map((entry) => path.relative(log.workspace, resolveToolPath(ctx.cwd, entry)).split(path.sep).join('/'));
        const changes = log?.changes({ withinMs: minutes * 60_000, ...(prefixes?.length ? { prefixes } : {}) }) ?? [];
        const text = describeChanges(changes, { self: team.id, minutes, limit: params.limit ?? DEFAULT_CHANGES_LIMIT, paths: params.paths }, now);
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
      const id = (args as { id?: unknown }).id;
      const summary = [String(args.action ?? ''), id === undefined ? '' : String(id), target, args.note ? `"${String(args.note)}"` : ''].filter(Boolean).join(' ');
      return toolHeader(theme, context, 'Coordinate', summary);
    },
    renderResult(result, _options, theme, context) {
      const body = result.content.map((part) => (part.type === 'text' ? part.text : '')).join('\n');
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
      'Send task steering, a question or useful evidence to a live agent in this repository; "all" reaches every other agent in your session tree (the top session and its subagents). Messages are queued: busy recipients read them at their next step; if a recipient leaves first, you are told it went unread. Questions, and the answer to a question the recipient asked, wake idle recipients; FYIs and other replies wait for their next turn. ' +
      'Direct messages request a reply by default; broadcasts and replies do not. Use replyRequired false for an FYI and replyTo for an existing message. Oversized messages are rejected; send a summary and full-content path. Finished subagents need a new agent task to continue.',
    promptSnippet: 'Steer a worker, ask a peer or share decision-changing evidence',
    promptGuidelines: ['Send self-contained messages when they change another agent\'s next action. Reply in the existing thread with replyTo; routine acknowledgements and completion pings duplicate automatic reports.'],
    parameters: Type.Object({
      to: Type.String({ description: 'Agent id, role, "parent" or "all" (your session tree)' }),
      message: Type.String({ maxLength: MESSAGE_MAX_CHARS, description: 'Self-contained text; for long results send a file path' }),
      replyRequired: Type.Optional(Type.Boolean({ description: 'Request an answer; false for FYI. Default true for direct messages, false for broadcasts and replies' })),
      replyTo: Type.Optional(Type.Integer({ minimum: 1, description: 'Number of an existing message being answered; use its sender as to' })),
    }),
    async execute(_id, params, signal) {
      signal?.throwIfAborted();
      team.join();
      const result = team.send(params.to, params.message, {
        ...(params.replyRequired !== undefined ? { replyRequired: params.replyRequired } : {}),
        ...(params.replyTo !== undefined ? { replyTo: params.replyTo } : {}),
      });
      if ('error' in result) throw new Error(result.error);
      const departed = result.departed?.length ? ` Not sent to ${result.departed.join(', ')} (left the team).` : '';
      return textResult(`Queued message #${result.id} for ${result.sent.join(', ')}.${departed}`, { to: result.sent, id: result.id, ...(result.departed?.length ? { departed: result.departed } : {}) });
    },
    renderCall(args, theme, context) {
      const to = args.to === undefined ? '' : `→ ${String(args.to)}`;
      return toolHeader(theme, context, 'Send', args.replyTo !== undefined ? `${to} · reply to #${String(args.replyTo)}` : to);
    },
    renderResult(result, _options, theme, context) {
      const details: Record<string, unknown> = isRecord(result.details) ? result.details : {};
      const text = result.content.map((part) => (part.type === 'text' ? part.text : '')).join('\n');
      const sent = Array.isArray(details['to']) ? details['to'].map(String) : [];
      const departed = Array.isArray(details['departed']) && details['departed'].length ? ` · not sent to ${details['departed'].map(String).join(', ')}` : '';
      const summary = typeof details['id'] === 'number' ? `Queued #${details['id']}${sent.length > 1 ? ` to ${plural(sent.length, 'agent')}` : ''}${departed}` : firstLine(text);
      // The message itself is the model's own words: shown only when expanded.
      const message = isRecord(context.args) ? String(context.args['message'] ?? '') : '';
      return resultBlock(theme, context, { summary, body: context.isError ? text.split('\n').slice(1).join('\n') : message, max: context.isError ? 3 : 0, ...timingOf(result.details) });
    },
  }));
}

function registerTeamEvents(pi: ExtensionAPI, team: Team): void {
  pi.on('session_start', async (_event, ctx) => team.start(ctx));
  pi.on('session_shutdown', async () => team.stop());
  pi.on('agent_start', async () => team.onAgentStart());
  pi.on('agent_settled', async () => team.onAgentSettled());
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
  pi.on('message_end', async (event) => {
    if (event.message.role === 'assistant') team.onUsage(event.message.usage);
  });
}
